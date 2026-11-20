# Source repositories and builds

| Repository | Responsibility |
| --- | --- |
| `octoii-works/octoii` | Product host, extensions, Node, Web and desktop apps |
| `octoii-works/octoii-ai` | Public `octoii-ai` crate for the model protocol, providers and transport |
| `octoii-works/octoii-siphon-lib` | Public Rust SDK and platform-specific Siphon binaries |
| `devxiyang/octoii-siphon` | Private Siphon implementation and independent Beak workspace |
| `devxiyang/octoii-mobile` | Private iPhone client prototype and its build |

Cargo resolves the public crates using `Cargo.lock`. The product takes its native Siphon binary from the
SDK release of the same version. The installer selects the Rust host target and verifies the archive's ABI,
target and file checksums. A standard product checkout uses these public artifacts.

## Build Octoii

Follow the [native build instructions](../CONTRIBUTING.md#native-build-dependency) from the product checkout.

```sh
export SIPHON_LIB_DIR="$(python3 tools/install-siphon.py)"
cargo build --locked -p octoii-node --bin octoii
```

Use the same published binary for workspace tests. The dependency check verifies that only the product host
directly uses the SDK and that the Cargo graph contains no private Siphon or Beak packages. CI installs the
public native library before testing the product. See [package boundaries](ARCHITECTURE.md) for the current
SDK, ABI and volume format.

## Update a dependency

Publish and verify the AI crate or the paired Siphon SDK and native artifacts first. Then update the relevant
entry in the product's `Cargo.toml` and `Cargo.lock`, build against the matching release, and run the product
checks. The AI, Siphon and SDK repositories maintain their own implementation and release checks.

The [extraction notes](SIPHON_EXTRACTION.md) describe the earlier source checkout. The
[binary distribution notes](SIPHON_DISTRIBUTION.md) record the transition to published artifacts.
