// One-time initialization of the NetRust wasm module, shared by all pages.
import init, * as wasm from '../pkg/netrust_wasm.js';

let ready = null;

export function loadWasm() {
  if (!ready) ready = init().then(() => wasm);
  return ready;
}
