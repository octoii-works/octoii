/** A local directory command palette: type a path, descend into a suggestion, or add the directory. */
import { type ReactNode, useRef, useState } from "react";
import { useClient } from "@octoii-works/octoii-client/react";
import { Folder } from "lucide-react";
import { childPath, foldersAmong } from "../../core/files.ts";
import { useFolder } from "../effects/folder.ts";
import { failureOf } from "../effects/read.ts";
import { Button } from "../ui/button.tsx";
import { Command, CommandDialog, CommandSearch, CommandList, CommandGroup, CommandItem, CommandHints } from "../ui/command.tsx";

type FolderChooserProps = {
	readonly open: boolean;
	readonly onOpenChange: (open: boolean) => void;
	readonly start: string | undefined;
	readonly onChoose: (folder: string) => void;
};

export function FolderChooser(props: FolderChooserProps): ReactNode {
	return (
		<CommandDialog open={props.open} onOpenChange={props.onOpenChange} title="Add local project">
			{props.open ? <DirectoryPicker start={props.start} onChoose={props.onChoose} /> : null}
		</CommandDialog>
	);
}

function DirectoryPicker(props: Pick<FolderChooserProps, "start" | "onChoose">): ReactNode {
	const client = useClient();
	const input = useRef<HTMLInputElement>(null);
	const [path, setPath] = useState(props.start === undefined ? "~/" : props.start.replace(/\/$/, "") + "/");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string>();
	const slash = path.lastIndexOf("/");
	const parent = slash < 0 ? "~" : path.slice(0, slash + 1);
	const search = path.slice(slash + 1);
	const folder = useFolder(parent);
	const read = folder !== undefined && "path" in folder ? folder : undefined;
	const entries =
		read === undefined
			? []
			: foldersAmong(read.entries, search.startsWith("."))
					.filter((entry) => entry.name.toLocaleLowerCase().startsWith(search.toLocaleLowerCase()))
					.map((entry) => ({ name: entry.name, path: childPath(read.path, entry.name) }));
	const items =
		read !== undefined && read.path !== "/" && search === ""
			? [{ name: "..", path: read.path.slice(0, read.path.lastIndexOf("/")) || "/" }, ...entries]
			: entries;
	const change = (value: string): void => {
		setPath(value);
		setError(undefined);
	};
	const enter = (value: string): void => {
		if (busy) return;
		change(value.replace(/\/$/, "") + "/");
		input.current?.focus();
	};
	const add = async (): Promise<void> => {
		if (busy || path.trim() === "") return;
		setBusy(true);
		setError(undefined);
		try {
			const directory = await client.call("files.list", { path: path.trim(), limit: 1 });
			props.onChoose(directory.path);
		} catch (failure) {
			setError(failureOf(failure));
		} finally {
			setBusy(false);
		}
	};
	return (
		<fieldset className="min-w-0 border-0 p-0">
			<Command
				items={[{ items }]}
				mode="none"
				autoHighlight={false}
				value={path}
				onValueChange={change}
				itemToStringValue={(item: { name: string }) => item.name}
			>
				<CommandSearch
					ref={input}
					readOnly={busy}
					aria-label="Project path"
					placeholder="~/path/to/project"
					className="font-mono text-ui"
					onKeyDown={(event) => {
						if (event.key === "Enter" && (event.metaKey || event.ctrlKey || !event.currentTarget.getAttribute("aria-activedescendant"))) {
							event.preventDefault();
							event.stopPropagation();
							void add();
						}
					}}
					aside={
						<Button size="sm" disabled={busy || path.trim() === ""} onClick={() => void add()}>
							{busy ? "Adding…" : "Add"}
						</Button>
					}
				/>
				<CommandList empty={folder === undefined ? "Reading directories…" : read === undefined ? folder.failure : "No matching directories"}>
					<CommandGroup title="Directories" items={items}>
						{(item) => (
							<CommandItem key={item.path} value={item} onClick={() => enter(item.path)} className="h-control">
								<Folder className="size-4 shrink-0 text-muted-foreground" />
								<span className="truncate">{item.name}</span>
							</CommandItem>
						)}
					</CommandGroup>
				</CommandList>
			</Command>
			{read?.readMore === undefined ? null : (
				<button type="button" onClick={read.readMore} className="px-4 py-2 text-xs text-muted-foreground">
					Show more directories
				</button>
			)}
			{error === undefined ? null : (
				<p role="alert" className="break-words px-4 py-2 text-xs text-error-foreground">
					{error}
				</p>
			)}
			<CommandHints
				hints={[
					{ keys: ["↑", "↓"], label: "Navigate" },
					{ keys: ["Enter"], label: "Open / Add" },
					{ keys: ["Esc"], label: "Close" },
				]}
			/>
		</fieldset>
	);
}
