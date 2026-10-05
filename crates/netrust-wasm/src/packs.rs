//! Rule pack bindings: load/build packs in the browser, inspect, diff, export.

use std::sync::Arc;

use netrust_data::ruleset::{Ruleset, RulesetRef};
use netrust_pack::{
    build_from_files, diff, load_nrpack_bytes, nrpack_to_bytes, parse_pack_files, resolve,
    ruleset_hash, validate, vanilla_pack_files, NrPack, PackError, PackFiles, Report,
    PACK_FORMAT_VERSION,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmPack {
    pub(crate) nrpack: NrPack,
    pub(crate) ruleset: Arc<Ruleset>,
    pub(crate) rref: RulesetRef,
    report: Report,
}

fn err_text(e: PackError) -> String {
    match e {
        PackError::Invalid(r) => {
            let first: Vec<String> = r
                .errors
                .iter()
                .take(5)
                .map(|d| {
                    format!(
                        "{}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    )
                })
                .collect();
            format!(
                "Validation failed with {} error(s): {}",
                r.errors.len(),
                first.join("; ")
            )
        }
        other => other.to_string(),
    }
}

fn files_from_json(files_json: &str) -> Result<PackFiles, String> {
    serde_json::from_str::<std::collections::BTreeMap<String, String>>(files_json)
        .map(PackFiles)
        .map_err(|e| format!("files must be a JSON object of path -> text: {e}"))
}

impl WasmPack {
    fn from_nrpack(nrpack: NrPack, report: Report) -> Self {
        let mut rs = nrpack.ruleset.clone();
        rs.reindex();
        let rref = RulesetRef {
            id: nrpack.manifest.id.clone(),
            version: nrpack.manifest.version.clone(),
            hash: nrpack.hash.clone(),
        };
        Self {
            nrpack,
            ruleset: Arc::new(rs),
            rref,
            report,
        }
    }

    pub fn from_files_impl(files_json: &str) -> Result<WasmPack, String> {
        let files = files_from_json(files_json)?;
        let (p, report) = build_from_files(&files).map_err(err_text)?;
        Ok(Self::from_nrpack(p, report))
    }

    pub fn from_nrpack_impl(bytes: &[u8]) -> Result<WasmPack, String> {
        let (_, _, p) = load_nrpack_bytes(bytes).map_err(err_text)?;
        Ok(Self::from_nrpack(p, Report::default()))
    }
}

/// Validation report for pack files even when the build fails (for the editor).
pub fn report_for_files_impl(files_json: &str) -> String {
    let report = match files_from_json(files_json) {
        Err(msg) => Report {
            errors: vec![diag("files", &msg)],
            warnings: vec![],
        },
        Ok(files) => {
            match parse_pack_files(&files).and_then(|pd| resolve(&pd, &Ruleset::vanilla())) {
                Err(PackError::Resolve(d)) => Report {
                    errors: vec![d],
                    warnings: vec![],
                },
                Err(e) => Report {
                    errors: vec![diag("pack", &e.to_string())],
                    warnings: vec![],
                },
                Ok(rs) => match build_from_files(&files) {
                    Ok((_, r)) => r,
                    Err(PackError::Invalid(r)) => r,
                    Err(_) => validate(&rs),
                },
            }
        }
    };
    serde_json::to_string(&report).expect("report json")
}

fn diag(file: &str, msg: &str) -> netrust_pack::Diagnostic {
    netrust_pack::Diagnostic {
        file: file.into(),
        entry: String::new(),
        field: None,
        message: msg.into(),
    }
}

#[wasm_bindgen]
impl WasmPack {
    #[wasm_bindgen(js_name = fromFiles)]
    pub fn from_files(files_json: &str) -> Result<WasmPack, JsError> {
        Self::from_files_impl(files_json).map_err(|e| JsError::new(&e))
    }

    #[wasm_bindgen(js_name = fromNrpack)]
    pub fn from_nrpack_js(bytes: &[u8]) -> Result<WasmPack, JsError> {
        Self::from_nrpack_impl(bytes).map_err(|e| JsError::new(&e))
    }

    pub fn vanilla() -> WasmPack {
        let rs = Ruleset::vanilla();
        let ruleset = (*rs).clone();
        let hash = ruleset_hash(&ruleset);
        let nrpack = NrPack {
            format: PACK_FORMAT_VERSION,
            manifest: ruleset.manifest.clone(),
            ruleset,
            hash,
        };
        let rref = RulesetRef::vanilla();
        Self {
            nrpack,
            ruleset: rs,
            rref,
            report: Report::default(),
        }
    }

    #[wasm_bindgen(js_name = manifestJson)]
    pub fn manifest_json(&self) -> String {
        serde_json::to_string(&self.nrpack.manifest).expect("manifest json")
    }

    pub fn hash(&self) -> String {
        self.nrpack.hash.clone()
    }

    #[wasm_bindgen(js_name = reportJson)]
    pub fn report_json(&self) -> String {
        serde_json::to_string(&self.report).expect("report json")
    }

    #[wasm_bindgen(js_name = rulesetJson)]
    pub fn ruleset_json(&self) -> String {
        serde_json::json!({
            "monsters": self.nrpack.ruleset.monsters,
            "items": self.nrpack.ruleset.items,
            "roles": self.nrpack.ruleset.roles,
        })
        .to_string()
    }

    #[wasm_bindgen(js_name = diffVsVanillaJson)]
    pub fn diff_vs_vanilla_json(&self) -> String {
        serde_json::to_string(&diff(&Ruleset::vanilla(), &self.nrpack.ruleset)).expect("diff json")
    }

    #[wasm_bindgen(js_name = nrpackBytes)]
    pub fn nrpack_bytes(&self) -> Vec<u8> {
        nrpack_to_bytes(&self.nrpack)
    }
}

#[wasm_bindgen(js_name = vanillaPackFilesJson)]
pub fn vanilla_pack_files_json() -> String {
    serde_json::to_string(&vanilla_pack_files().0).expect("files json")
}

#[wasm_bindgen(js_name = reportForFilesJson)]
pub fn report_for_files_json(files_json: &str) -> String {
    report_for_files_impl(files_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hard_mode_files_json() -> String {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/examples/hard-mode");
        let files = netrust_pack::read_pack_files_from_dir(&dir).unwrap();
        serde_json::to_string(&files.0).unwrap()
    }

    #[test]
    fn from_files_builds_and_matches_cli_hash() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/examples/hard-mode");
        let (cli, _) = netrust_pack::build(&dir).unwrap();
        let p = WasmPack::from_files_impl(&hard_mode_files_json()).unwrap();
        assert_eq!(p.hash(), cli.hash);
        assert_eq!(p.nrpack_bytes(), netrust_pack::nrpack_to_bytes(&cli));
    }

    #[test]
    fn nrpack_roundtrip_and_views() {
        let p = WasmPack::from_files_impl(&hard_mode_files_json()).unwrap();
        let q = WasmPack::from_nrpack_impl(&p.nrpack_bytes()).unwrap();
        assert_eq!(p.hash(), q.hash());
        let rs: serde_json::Value = serde_json::from_str(&q.ruleset_json()).unwrap();
        assert!(rs["monsters"].as_array().unwrap().len() > 10);
        let diff: serde_json::Value = serde_json::from_str(&q.diff_vs_vanilla_json()).unwrap();
        assert!(!diff.as_array().unwrap().is_empty());
        let rep: serde_json::Value = serde_json::from_str(&q.report_json()).unwrap();
        assert!(rep["errors"].as_array().unwrap().is_empty());
        let man: serde_json::Value = serde_json::from_str(&q.manifest_json()).unwrap();
        assert!(man["id"].is_string());
    }

    #[test]
    fn vanilla_has_empty_diff() {
        let v = WasmPack::vanilla();
        assert_eq!(v.diff_vs_vanilla_json(), "[]");
        assert!(v.hash().starts_with("sha256:"));
    }

    #[test]
    fn from_nrpack_rejects_garbage() {
        assert!(WasmPack::from_nrpack_impl(b"{\"format\":1").is_err());
        assert!(WasmPack::from_nrpack_impl(&[0, 159, 146, 150]).is_err());
    }

    #[test]
    fn from_files_reports_toml_error_with_file_name() {
        let err = WasmPack::from_files_impl(r#"{"pack.toml":"id = "}"#).unwrap_err();
        assert!(err.contains("pack.toml"), "{err}");
    }

    #[test]
    fn invalid_pack_still_returns_report() {
        // A pack that fails validation must still be inspectable: from_files
        // returns Err with the report text, and report_for_files gives JSON.
        let files = r#"{"pack.toml":"id = \"x\"\nname = \"X\"\nversion = \"1.0.0\"\nbase = \"vanilla\"\n","monsters.toml":"[[monster]]\nname = \"jackal\"\nlevel = -5\n"}"#;
        let rep: serde_json::Value = serde_json::from_str(&report_for_files_impl(files)).unwrap();
        assert!(!rep["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn vanilla_pack_files_json_parses() {
        let m: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&vanilla_pack_files_json()).unwrap();
        assert!(m.contains_key("monsters.toml"));
    }

    #[test]
    fn game_session_starts_with_pack() {
        let p = WasmPack::from_files_impl(&hard_mode_files_json()).unwrap();
        let s =
            crate::WasmGameSession::new_with_pack(7, "valkyrie", "human", "Tester", &p).unwrap();
        assert_eq!(s.ruleset_hash(), p.hash());
        let obs: serde_json::Value = serde_json::from_str(&s.get_observation_json()).unwrap();
        assert!(obs.is_object());
    }
}
