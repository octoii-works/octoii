# AGENTS.md

- **Clean Code**: clear names, small functions, errors handled explicitly. No flag parameters: a boolean that picks
  between two behaviors makes two functions; a boolean that is a value — a mode being set, an option's field — is data.
- **KISS**: the simplest design that works.
- **YAGNI**: build nothing before it is needed; no compatibility shims, change the API and migrate the callers.
- **Design, not patches**: design from the architecture, not from the symptom — find the concept that should hold a
  need and change the design until the need is a plain case of it; never bolt on a special case, a flag, an override
  layer or a fallback. One concept, one place: where two mechanisms do nearly the same job, make them one.
- **Code style**: read like the code around it.
- **Core stability**: treat `core/` as frozen by default; build product capabilities in extensions and apps using
  existing public interfaces. Before changing any file under `core/` (including tests and manifests), explain the
  need, exact scope, and why an extension-only solution is insufficient, then obtain a separate explicit confirmation
  for that core change. General approval to implement a feature or refactor is not this confirmation. Read-only
  investigation and checks are allowed; do not bypass core boundaries to avoid asking.
- **Upper-layer boundary**: apps, product services, extensions and their shared libraries interact with Siphon through
  its public interfaces, not directly with Beak or `siphon-runtime`. Direct lower-layer access is an exception only
  where genuinely necessary: document why Siphon's public interface cannot serve the need and keep the access in the
  smallest possible boundary, such as node initialization. An existing direct dependency is not permission to add
  more. Do not expose kernel internals or bypass Siphon's state, permissions or execution rules for convenience.
  Changes to `core/` still require the separate confirmation above.
- **Extensions**: an extension reaches Siphon by its public interface alone, never Beak or `siphon-runtime`, and writes
  the volume only through Siphon's primitives (`Volume`, `machine::Commands`, a plan's spawn and deliver). What an
  extension needs that Siphon lacks becomes a primitive of Siphon's, never a way around it.
- **This file** changes only with the user's confirmation.
