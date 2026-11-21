import { type ReactNode, useRef, useState } from "react";
import type { PageDownload, PageFileChooser } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Download, FileUp } from "lucide-react";
import { readDownload, saveDownload } from "../../components/files/file-download.ts";
import { failed } from "../../components/state/notices.ts";
import { useReach } from "../../components/state/reach.ts";
import { Button } from "../../components/ui/button.tsx";
import { MenuItem, MenuItemLabel, MenuPopup, MenuSub, MenuSubTrigger } from "../../components/ui/menu.tsx";

/** A file input opened by the page. The files selected here come from the controlling device. */
export function FileChooserBar(props: {
	readonly page: string;
	readonly thread: string;
	readonly chooser: PageFileChooser;
	readonly disabled: boolean;
}): ReactNode {
	const client = useClient();
	const input = useRef<HTMLInputElement>(null);
	const [working, setWorking] = useState(false);
	const choose = async (files: readonly File[]): Promise<void> => {
		if (props.disabled || working) return;
		setWorking(true);
		try {
			const kept = [];
			for (const file of files) kept.push(await client.keepFile(props.thread, file, file.name));
			await client.call("browser.choose_files", { page: props.page, chooser: props.chooser.id, files: kept });
		} catch (error) {
			failed("The files could not be selected", error);
		} finally {
			setWorking(false);
		}
	};
	return (
		<div role="group" aria-label="Page file chooser" className="flex shrink-0 items-center justify-end gap-2 border-b border-border bg-accent px-3 py-2">
			<input
				ref={input}
				type="file"
				aria-label="Files for this page"
				multiple={props.chooser.multiple}
				disabled={props.disabled || working}
				hidden
				onChange={(event) => {
					const picked = Array.from(event.currentTarget.files ?? []);
					event.currentTarget.value = "";
					void choose(picked);
				}}
			/>
			<Button variant="ghost" size="xs" disabled={props.disabled || working} onClick={() => void choose([])}>
				Cancel
			</Button>
			<Button variant="outline" size="xs" disabled={props.disabled || working} onClick={() => input.current?.click()}>
				<FileUp className="size-3.5" />
				{working ? "Selecting…" : props.chooser.multiple ? "Choose files" : "Choose file"}
			</Button>
		</div>
	);
}

/** Downloads initiated by the page or its popup. The saved file uses the thread's original-file route. */
export function BrowserDownloads(props: { readonly page: string; readonly thread: string; readonly downloads: readonly PageDownload[] }): ReactNode {
	const client = useClient();
	const { send } = useReach();
	const [saving, setSaving] = useState<string>();
	const save = async (download: string): Promise<void> => {
		setSaving(download);
		try {
			const file = await client.call("browser.keep_download", { page: props.page, download });
			saveDownload(await readDownload(send, props.thread, file));
		} catch (error) {
			failed("The download could not be saved", error);
		} finally {
			setSaving(undefined);
		}
	};
	return (
		<MenuSub>
			<MenuSubTrigger>
				<Download />
				<MenuItemLabel>Downloads</MenuItemLabel>
			</MenuSubTrigger>
			<MenuPopup side="right" className="w-56">
				{props.downloads.toReversed().map((item) => (
					<MenuItem key={item.id} disabled={item.state !== "completed" || saving !== undefined} onClick={() => void save(item.id)}>
						<MenuItemLabel>
							{item.state === "completed" ? item.name : `${item.name} · ${item.state === "inProgress" ? "Downloading…" : "Canceled"}`}
						</MenuItemLabel>
					</MenuItem>
				))}
			</MenuPopup>
		</MenuSub>
	);
}
