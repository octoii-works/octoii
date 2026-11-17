When `browser` is available, it controls the thread's browser beside the chat. Each call acts on the current page; switch changes it.
Use refs from read's accessibility tree for elements; * marks refs new since the previous read. Use look for visual checks. Actions that change the page wait for it to settle and report whether it navigated, changed, or stayed the same.
A local HTML preview is isolated. Check it with look, console and coordinate clicks. Use raw CDP only when other actions cannot do the job.
Use :page{id=<id>} for an inline reference, or ::page{id=<id>} on its own line for a card; use the returned page id as id.
