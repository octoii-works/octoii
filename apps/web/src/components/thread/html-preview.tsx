/** A saved HTML document, previewed inline or opened larger in the same isolated viewer. */
import { type ReactNode, useState } from "react";
import type { FileRef } from "@octoii-works/octoii-client";
import { Expand, X } from "lucide-react";
import { useHtmlFile, useHtmlFrame } from "../effects/html-preview.ts";
import { Button, IconButton } from "../ui/button.tsx";
import { Dialog, DialogClose, DialogPopup, DialogTitle } from "../ui/dialog.tsx";

/** Fetches the original with the app's authenticated client; the preview never receives credentials or a file URL. */
export function HtmlFile(props: { readonly thread: string; readonly file: FileRef }): ReactNode {
	const outcome = useHtmlFile(props.thread, props.file);
	if (outcome === undefined)
		return (
			<p role="status" className="text-xs text-muted-foreground">
				Loading preview…
			</p>
		);
	if ("failure" in outcome) {
		return (
			<p role="alert" className="text-xs text-error-foreground">
				Could not preview the file: {outcome.failure}
			</p>
		);
	}
	return <HtmlPreview key={`${props.thread}:${props.file.ref}`} html={outcome.result} name={props.file.filename ?? "file"} />;
}

/** A read document, shown as a frame and an optional larger dialog. */
export function HtmlPreview(props: { readonly html: string; readonly name: string }): ReactNode {
	const [opened, setOpened] = useState(false);
	return (
		<div className="flex min-w-0 flex-col gap-2">
			<HtmlFrame html={props.html} name={props.name} size="card" />
			<Button size="xs" variant="outline" className="self-end" aria-label={`Expand ${props.name}`} onClick={() => setOpened(true)}>
				<Expand />
				Expand
			</Button>
			<Dialog open={opened} onOpenChange={setOpened}>
				<DialogPopup className="w-[92vw] max-w-6xl gap-3 p-4">
					<div className="flex min-w-0 items-center justify-between gap-3">
						<DialogTitle className="min-w-0 truncate text-ui font-medium">{props.name}</DialogTitle>
						<DialogClose
							render={
								<IconButton label="Close preview">
									<X />
								</IconButton>
							}
						/>
					</div>
					{opened ? <HtmlFrame html={props.html} name={props.name} size="expanded" /> : null}
				</DialogPopup>
			</Dialog>
		</div>
	);
}

/** A separate HTTP document owns its CSP. Using srcdoc or a Blob here would inherit the app's script policy. */
function HtmlFrame(props: { readonly html: string; readonly name: string; readonly size: "card" | "expanded" }): ReactNode {
	const { frame, height, loaded, failure } = useHtmlFrame(props.html);
	if (failure !== undefined)
		return (
			<p role="alert" className="text-xs text-error-foreground">
				{failure}
			</p>
		);
	return (
		<div className="relative min-h-0 overflow-hidden rounded-md border border-border bg-background">
			<iframe
				ref={frame}
				src="/html-preview.html"
				sandbox="allow-scripts"
				referrerPolicy="no-referrer"
				title={`Preview of ${props.name}`}
				className={props.size === "expanded" ? "block h-[72vh] w-full border-0" : "block w-full border-0"}
				style={props.size === "card" ? { height } : undefined}
			/>
			{loaded ? null : (
				<p role="status" className="absolute inset-x-0 top-0 bg-background p-2 text-xs text-muted-foreground">
					Loading preview…
				</p>
			)}
		</div>
	);
}
