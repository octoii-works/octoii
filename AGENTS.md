# AGENTS.md

- **Clean Code**: clear names, small functions, errors handled explicitly. No flag parameters: a boolean that picks
  between two behaviors makes two functions; a boolean that is a value — a mode being set, an option's field — is data.
- **KISS**: the simplest design that works.
- **YAGNI**: build nothing before it is needed; no compatibility shims, change the API and migrate the callers.
- **Design, not patches**: design from the architecture, not from the symptom — find the concept that should hold a
  need and change the design until the need is a plain case of it; never bolt on a special case, a flag, an override
  layer or a fallback. One concept, one place: where two mechanisms do nearly the same job, make them one.
- **Code style**: read like the code around it.
- Keep inline Rust unit tests and their helpers in a `#[cfg(test)] mod tests` block at the end of the source file.
  Add new tests to the existing block.
- Reuse active Cargo build caches and remove temporary outputs after verification. Periodically review and prune
  build outputs that are no longer needed once their builds have stopped. Follow the
  [local verification cleanup guide](CONTRIBUTING.md#local-verification-cleanup).
- **Product core**: `core/` is the `octoii` Cargo package. It owns Thread, Turn, Agent execution and product
  rules. Apps compose Octoii and its extensions. Only `octoii` directly depends on the published Siphon SDK.
- **Upper-layer boundary**: apps, product services, extensions and their shared libraries interact with Siphon through
  its public interfaces, not directly with Beak or `siphon-runtime`. Direct lower-layer access is an exception only
  where genuinely necessary: document why Siphon's public interface cannot serve the need and keep the access in the
  smallest possible boundary, such as node initialization. An existing direct dependency is not permission to add
  more. Do not expose kernel internals or bypass Siphon's state, permissions or execution rules for convenience.
- **Extensions**: use Octoii's public product interfaces for Thread, tools, storage and machine operations. Put product
  rules in Octoii and general storage and execution mechanisms behind the public Siphon SDK.
- **This file** changes only with the user's confirmation.
