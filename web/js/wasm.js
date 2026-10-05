// One-time initialization of the NetHackED wasm module, shared by all pages.
import init, * as wasm from '../pkg/nethacked_wasm.js';

let ready = null;

export function loadWasm() {
  if (!ready) ready = init().then(() => wasm);
  return ready;
}
