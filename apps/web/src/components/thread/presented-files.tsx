/** Files a tool saved for the person: their title, original name and size, preview, and an authenticated download. */
import { type ReactNode, useState } from "react";
import { Download, FileText, LoaderCircle } from "lucide-react";
import { isImage, sizeLabel } from "../../core/attachments.ts";
import type { PresentedFile } from "../../core/presented-files.ts";
import { readDownload, saveDownload } from "../files/file-download.ts";
import { useReach } from "../state/reach.ts";
import { MenuItem } from "../ui/menu.tsx";
import { FileActions, FileMore } from "./file-actions.tsx";
import { HtmlFile } from "./html-preview.tsx";
import { PictureStrip } from "./pictures.tsx";

/** Explicit file deliveries, visible even while the call's details are folded. */
export function PresentedFiles(props: { readonly thread: string; readonly files: readonly PresentedFile[] }): ReactNode {
	if (props.files.length === 0) return null;
	return (
		<div className="work-indent flex flex-col items-start gap-2 py-2">
			{props.files.map((file, index) => (
				<FileCard key={`${file.file.ref}:${String(index)}`} thread={props.thread} presented={file} />
			))}
		</div>
	);
}

/** One delivery, with a raster image viewer or an isolated HTML preview where the file's type supports it. */
function FileCard(props: { readonly thread: string; readonly presented: PresentedFile }): ReactNode {
	const { presented, thread } = props;
	const { file, size, title } = presented;
	const reach = useReach();
	const [downloading, setDownloading] = useState(false);
	const [failure, setFailure] = useState<string | undefined>(undefined);
	const name = file.filename ?? "file";
	const download = async (): Promise<void> => {
		setDownloading(true);
		setFailure(undefined);
		try {
			saveDownload(await readDownload(reach.send, thread, file));
		} catch (error) {
			setFailure(error instanceof Error ? error.message : "Could not download the file.");
		} finally {
			setDownloading(false);
		}
	};
	const action = (
		<MenuItem
			aria-label={downloading ? `Downloading ${name}` : `Download ${name}`}
			disabled={downloading}
			onClick={() => {
				void download();
			}}
		>
			{downloading ? <LoaderCircle className="animate-spin" /> : <Download />}
			{downloading ? "Downloading…" : "Download original"}
		</MenuItem>
	);
	return (
		<div className="flex w-full flex-col gap-2">
			{title === undefined || file.media_type === "text/html" ? null : <p className="text-ui font-medium text-foreground">{title}</p>}
			{file.media_type === "text/html" ? (
				<HtmlFile thread={thread} file={file} actions={action} />
			) : (
				<div className="group/file relative w-full max-w-md">
					<div className="flex w-full max-w-md flex-col gap-2 rounded-md border border-border bg-muted p-3">
						<div className="flex min-w-0 items-center gap-2">
							<FileText className="size-5 shrink-0 text-muted-foreground" />
							<span className="flex min-w-0 flex-1 flex-col">
								<span title={name} className="truncate text-ui text-foreground">
									{name}
								</span>
								<span className="text-xs text-muted-foreground">{sizeLabel(size)}</span>
							</span>
						</div>
						{isImage(file.media_type) ? <PictureStrip pictures={[{ kind: "kept", ref: file.ref, name }]} /> : null}
					</div>
					<FileActions>
						<FileMore name={name}>{action}</FileMore>
					</FileActions>
				</div>
			)}
			{failure === undefined ? null : (
				<p role="alert" className="text-xs text-error-foreground">
					{failure}
				</p>
			)}
		</div>
	);
}
