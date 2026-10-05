//! NetHack Canonical ASCII Headstone & Epitaph Renderer.

/// Generates a canonical NetHack ASCII tombstone with centered inscriptions.
pub fn render_headstone(
    hero: &str,
    level: u32,
    depth: u32,
    killer: &str,
    date: &str,
    epitaph: &str,
) -> String {
    let width = 31;
    let center = |s: &str| -> String {
        let max_chars = width - 4;
        let trimmed: String = s.chars().take(max_chars).collect();
        let len = trimmed.chars().count();
        let pad_total = (width - 2).saturating_sub(len);
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        format!(
            "|{}{}{}|",
            " ".repeat(pad_left),
            trimmed,
            " ".repeat(pad_right)
        )
    };

    let killer_desc = if killer.starts_with("a ") || killer.starts_with("an ") {
        format!("killed by {killer}")
    } else {
        format!("killed by a {killer}")
    };

    let mut lines = Vec::new();
    lines.push("               +-----------------------------+".to_string());
    lines.push("              /                               \\".to_string());
    lines.push("             /                                 \\".to_string());
    lines.push(center("REST IN PEACE"));
    lines.push(center(""));
    lines.push(center(hero));
    lines.push(center(&format!("Level {level}")));
    lines.push(center(&format!("Died on Dlvl {depth}")));
    lines.push(center(&killer_desc));
    lines.push(center(""));
    if !epitaph.is_empty() {
        lines.push(center(epitaph));
    }
    lines.push(center(date));
    lines.push("             *                                 *".to_string());
    lines.push("            ***                               ***".to_string());
    lines.push("           *************************************".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headstone_multibyte_name_does_not_panic() {
        let name = "Святослав Хоробрий Великий Київський";
        let stone = render_headstone(name, 1, 1, "гоблін", "2026-10-04", "");
        let line = stone.lines().find(|l| l.contains("Святослав")).unwrap();
        assert_eq!(line.chars().count(), 31);
    }

    #[test]
    fn test_headstone_rendering() {
        let stone = render_headstone(
            "Conan",
            5,
            3,
            "hill orc",
            "2026-10-04",
            "Always fought bravely",
        );
        assert!(stone.contains("REST IN PEACE"));
        assert!(stone.contains("Conan"));
        assert!(stone.contains("Level 5"));
        assert!(stone.contains("Died on Dlvl 3"));
        assert!(stone.contains("killed by a hill orc"));
        assert!(stone.contains("Always fought bravely"));
        assert!(stone.contains("2026-10-04"));
        assert!(stone.contains("*************************************"));
    }
}
