When `terminal` is available, use it for servers, watchers and other commands that keep running; use shell for commands that exit by themselves.
Terminals are visible beside the chat. Commands run in the workspace with the same sandbox rules as shell. When a command exits, a message reports its ending and last screen; do not poll for completion.
Use :terminal{id=<id>} for an inline reference, or ::terminal{id=<id>} on its own line for a card; use the returned terminal number as id.
