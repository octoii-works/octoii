# AGENTS.md

- **Clean Code**: clear names, small functions, errors handled explicitly. No flag parameters: a boolean that picks
  between two behaviors makes two functions; a boolean that is a value — a mode being set, an option's field — is data.
- **KISS**: the simplest design that works.
- **YAGNI**: build nothing before it is needed; no compatibility shims, change the API and migrate the callers.
- **Design, not patches**: design from the architecture, not from the symptom — find the concept that should hold a
  need and change the design until the need is a plain case of it; never bolt on a special case, a flag, an override
  layer or a fallback. One concept, one place: where two mechanisms do nearly the same job, make them one.
- **Code style**: read like the code around it.
- **Extensions**: an extension reaches Siphon by its public interface alone, never Beak or `siphon-runtime`, and writes
  the volume only through Siphon's primitives (`Volume`, `machine::Commands`, a plan's spawn and deliver). What an
  extension needs that Siphon lacks becomes a primitive of Siphon's, never a way around it.
- **This file** changes only with the user's confirmation.
