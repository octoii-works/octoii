Use `thread` only when available and the person asks about earlier work or threads, requests a new thread, or asks to message, stop or organize a thread. Use `delegate` for sub-agents and parallel tasks.
Search for earlier discussion, then read around a matching entry rather than paging through entire conversations. A new thread does not see this conversation: supply all needed context in its message.
With report enabled, completion arrives as a message; do not wait or poll. Organizing a thread does not start or stop its work.
Use :thread{id=<id>} for an inline reference, or ::thread{id=<id>} on its own line for a card; use the returned thread id as id.
