#!/usr/bin/env bash
# Builds the tunnel a page runs into vendor/tunnel/: the tunnel's WebAssembly at the commit the node is pinned to, so the
# web app and the node speak one version — bound by wasm-bindgen, of the version the tunnel's lockfile names, and
# optimized by wasm-opt. The tunnel's source is fetched into ~/.cache/octoii once.
#
#   apps/web/scripts/vendor-tunnel.sh

set -euo pipefail

WEB="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PINNED="$(sed -n 's/^octoii-tunnel = .*rev = "\([0-9a-f]*\)".*/\1/p' "${WEB}/../../crates/octoii/Cargo.toml")"
TUNNEL="${HOME}/.cache/octoii/octoii-tunnel"

[[ -d "${TUNNEL}/.git" ]] || git clone --filter=blob:none https://github.com/octoii-works/octoii-tunnel.git "${TUNNEL}"
if [[ "$(git -C "${TUNNEL}" rev-parse HEAD)" != "${PINNED}" ]]; then
	git -C "${TUNNEL}" fetch origin "${PINNED}"
	git -C "${TUNNEL}" checkout --detach "${PINNED}"
fi

(cd "${TUNNEL}" && cargo build -q -p octoii-tunnel-wasm --target wasm32-unknown-unknown --profile wasm)
out="$(mktemp -d)"
trap 'rm -rf "${out}"' EXIT
wasm-bindgen --target web --out-dir "${out}" "${TUNNEL}/target/wasm32-unknown-unknown/wasm/octoii_tunnel_wasm.wasm"
wasm-opt -Oz --strip-debug --strip-producers --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
	--enable-mutable-globals --enable-reference-types --enable-multivalue \
	-o "${WEB}/vendor/tunnel/octoii_tunnel_wasm_bg.wasm" "${out}/octoii_tunnel_wasm_bg.wasm"
install -m 644 "${out}/octoii_tunnel_wasm.js" "${out}/octoii_tunnel_wasm.d.ts" "${out}/octoii_tunnel_wasm_bg.wasm.d.ts" "${WEB}/vendor/tunnel/"
chmod 0644 "${WEB}/vendor/tunnel/octoii_tunnel_wasm_bg.wasm"
echo "Built vendor/tunnel/ (the tunnel ${PINNED:0:9})"
