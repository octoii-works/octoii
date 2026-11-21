/**
 * A page of the thread's browser, filling its tab of the right panel: a bar to go back, forward and load it again, and
 * its address, which takes what is typed — an address, a host, or words searched for — as Enter is pressed, and shows
 * where the page is otherwise; under it the dialog that holds the page, where one does, to answer; below them the page,
 * drawn and driven as its view. A blank page waits with the address bar focused.
 */
import { type FormEvent, type ReactNode, useState } from "react";
import type { PageDialog, PageListed } from "@octoii-works/octoii-client";
import { useClient, useConnection } from "@octoii-works/octoii-client/react";
import {
	ArrowLeft,
	ArrowRight,
	Braces,
	Ellipsis,
	ExternalLink,
	Globe,
	LoaderCircle,
	MessageSquareWarning,
	MonitorSmartphone,
	MousePointer2,
	RotateCw,
	Search,
	ZoomIn,
	ZoomOut,
} from "lucide-react";
import { addressOf, shownAddress } from "../../core/browser.ts";
import { systemBrowserAddress } from "../../core/links.ts";
import { shortcutLabel } from "../../core/shortcuts.ts";
import { FIND_SHORTCUT, useBrowserFind } from "../../components/effects/browser-find.ts";
import { usePageView } from "../../components/effects/page.ts";
import { useElementPicker } from "../../components/effects/element-pick.ts";
import { useBrowserLayout } from "../../components/effects/browser-layout.ts";
import { Button, IconButton } from "../../components/ui/button.tsx";
import { Menu, MenuItem, MenuItemLabel, MenuPopup, MenuSeparator, MenuShortcut, MenuTrigger } from "../../components/ui/menu.tsx";
import { failed } from "../../components/state/notices.ts";
import { AnnotationEditor } from "./annotation.tsx";
import { useDesktopPage, useLocalBrowserNode } from "./desktop.ts";
import { BrowserDownloads, FileChooserBar } from "./files.tsx";
import { FindBar } from "./find-bar.tsx";
import { ViewportControls } from "./viewport-controls.tsx";
import { ViewportSurface } from "./viewport-surface.tsx";

/** The page `page`. */
export function BrowserPane(props: { readonly thread: string; readonly page: PageListed; readonly onSent: (delivery: string) => void }): ReactNode {
	const { page } = props;
	const connected = useConnection().status === "open";
	const picker = useElementPicker(props.thread, page);
	const driving = page.control === "you";
	const desktop = useDesktopPage(page.id);
	const presentation = desktop === undefined ? "stream" : "native";
	const selecting = picker.mode !== "browse";
	const disconnected = page.load.state === "disconnected";
	const problem = problemOf(page, connected);
	const canNavigate = connected && !disconnected && driving && !selecting;
	const layout = useBrowserLayout(page, presentation, canNavigate);
	const interaction = problem === undefined && (selecting || (canNavigate && !layout.saving)) ? picker : { ...picker, mode: "observe" as const };
	const { mount, ready, focus } = usePageView(page, interaction, presentation, layout.room.size);
	const canConfigure = layout.editable && !layout.saving;
	const canPick = ready && !layout.saving && page.load.state === "ready" && page.dialog === undefined;
	const find = useBrowserFind(canNavigate && page.load.state === "ready" && page.dialog === undefined ? desktop?.guest : undefined, page.url);
	return (
		<div className="flex min-h-0 flex-1 flex-col" onKeyDownCapture={find.shortcut}>
			<div className="flex h-control-lg shrink-0 items-center gap-0.5 border-b border-border px-2 text-xs">
				<PageNavigation page={page} enabled={canNavigate} />
				{layout.resizable ? (
					<IconButton
						label={page.viewport === "fixed" ? "Fit viewport to panel" : "Responsive viewport"}
						aria-pressed={page.viewport === "fixed"}
						disabled={!canConfigure}
						onClick={() =>
							page.viewport === "fixed" ? layout.resize("fill", layout.room.size ?? page.viewport_size) : layout.resize("fixed", page.viewport_size)
						}
					>
						<MonitorSmartphone className="size-3.5" />
					</IconButton>
				) : null}
				<IconButton
					label={selecting ? "Cancel element selection" : "Select an element for your message"}
					aria-pressed={selecting}
					disabled={problem !== undefined || (!selecting && !canPick)}
					onClick={() => (selecting ? picker.cancel() : picker.start())}
				>
					{picker.mode === "capturing" ? <LoaderCircle className="size-3.5 animate-spin" /> : <MousePointer2 className="size-3.5" />}
				</IconButton>
				<PageControlButton page={page} enabled={connected && !disconnected && !selecting && ready} />
				<span
					className="shrink-0 px-1 text-xs text-muted-foreground"
					title={page.host === "desktop" ? "Uses the desktop’s browser session" : "Uses the node’s browser session"}
				>
					{page.host === "desktop" ? "Desktop" : "Node"}
				</span>
				<BrowserOptions
					thread={props.thread}
					page={page}
					native={desktop !== undefined}
					connected={connected}
					editable={canConfigure}
					onFind={find.open}
					onZoom={layout.zoomPage}
				/>
			</div>
			<FindBar
				find={find}
				onClose={() => {
					find.close();
					focus();
				}}
			/>
			{layout.fixed ? (
				<ViewportControls
					size={page.viewport_size}
					zoom={layout.zoom}
					scale={layout.fit}
					editable={layout.editable}
					saving={layout.saving}
					onSize={(size) => layout.resize("fixed", size)}
					onZoom={layout.setZoom}
				/>
			) : null}
			<PagePrompts page={page} thread={props.thread} problem={problem} />
			<ViewportSurface mountRoom={layout.room.mount} desktop={desktop} preview={layout.preview} hidden={problem !== undefined}>
				<div ref={mount} className="pointer-events-auto absolute inset-0" />
				<SelectionOverlay picker={picker} ready={ready} onSent={props.onSent} />
			</ViewportSurface>
		</div>
	);
}

function PagePrompts({ page, thread, problem }: { readonly page: PageListed; readonly thread: string; readonly problem: Problem | undefined }): ReactNode {
	const client = useClient();
	if (problem !== undefined) return <PageProblem problem={problem} page={page} />;
	const disabled = page.control !== "you";
	return (
		<>
			{page.dialog === undefined ? null : (
				<DialogBar
					key={`${page.dialog.kind}:${page.dialog.message}`}
					dialog={page.dialog}
					disabled={disabled}
					onAnswer={(accept, text) =>
						void client
							.call("browser.dialog", { page: page.id, accept, ...(text === undefined ? {} : { text }) })
							.catch((error: unknown) => failed("The page's question could not be answered", error))
					}
				/>
			)}
			{page.chooser ? <FileChooserBar key={page.chooser.id} page={page.id} thread={thread} chooser={page.chooser} disabled={disabled} /> : null}
		</>
	);
}

function PageControlButton({ page, enabled }: { readonly page: PageListed; readonly enabled: boolean }): ReactNode {
	const client = useClient();
	const [changing, setChanging] = useState(false);
	const driving = page.control === "you";
	const change = (): void => {
		setChanging(true);
		void client
			.call("browser.control", { page: page.id, action: driving ? "release" : "take" })
			.catch((error: unknown) => failed(driving ? "Control could not be returned" : "The page could not be taken over", error))
			.finally(() => setChanging(false));
	};
	return (
		<Button variant="ghost" size="xs" disabled={!enabled || changing || page.control === "viewer"} onClick={change}>
			{driving ? "Return control" : page.control === "viewer" ? "Viewer has control" : "Take control"}
		</Button>
	);
}

function SelectionOverlay(props: {
	readonly picker: ReturnType<typeof useElementPicker>;
	readonly ready: boolean;
	readonly onSent: (delivery: string) => void;
}): ReactNode {
	const { mode, feedback, cancel } = props.picker;
	if (feedback) {
		return <AnnotationEditor key={`${feedback.screenshot.ref}:${String(feedback.captured_at)}`} feedback={feedback} onClose={cancel} onSent={props.onSent} />;
	}
	if (mode === "browse") return null;
	return (
		<div
			role="status"
			className="pointer-events-none absolute inset-x-3 top-3 z-10 rounded-md border border-border bg-background px-3 py-2 text-xs shadow-raised"
		>
			{mode === "capturing"
				? "Capturing the selected element…"
				: !props.ready
					? "Waiting for the page’s viewport…"
					: "Click an element to annotate it. Escape cancels."}
		</div>
	);
}

type Problem = { readonly title: string; readonly detail: string; readonly action?: string };

function problemOf(page: PageListed, connected: boolean): Problem | undefined {
	if (!connected) return { title: "Node disconnected", detail: "Waiting to reconnect to Octoii." };
	if (page.load.state === "failed") return { title: "This page could not be loaded", detail: page.load.message, action: "Retry" };
	if (page.load.state !== "disconnected") return undefined;
	if (page.host === "desktop" && window.octoiiBrowser === undefined) {
		return { title: "Browser disconnected", detail: "Reopen this page in Octoii on the desktop." };
	}
	return { title: "Browser disconnected", detail: "Reopen this page at its last address.", action: "Reopen page" };
}

function PageProblem({ problem, page }: { readonly problem: Problem; readonly page: PageListed }): ReactNode {
	const client = useClient();
	const [retrying, setRetrying] = useState(false);
	const retry = (): void => {
		setRetrying(true);
		void client
			.call("browser.retry", { page: page.id })
			.catch((error: unknown) => failed("The page could not be reopened", error))
			.finally(() => setRetrying(false));
	};
	return (
		<div role="status" className="grid min-h-0 flex-1 place-content-center gap-2 bg-muted px-5 text-center">
			<p className="text-ui font-medium">{problem.title}</p>
			<p className="text-xs text-muted-foreground">{problem.detail}</p>
			{problem.action === undefined ? null : (
				<Button className="mx-auto" size="xs" variant="outline" disabled={retrying || page.control === "viewer"} onClick={retry}>
					{retrying ? "Reopening…" : problem.action}
				</Button>
			)}
		</div>
	);
}

function PageNavigation({ page, enabled }: { readonly page: PageListed; readonly enabled: boolean }): ReactNode {
	const client = useClient();
	return (
		<>
			<IconButton
				label="Go back"
				disabled={!enabled || !page.back}
				onClick={() => void client.call("browser.back", { page: page.id }).catch((error: unknown) => failed("The page could not go back", error))}
			>
				<ArrowLeft className="size-3.5" />
			</IconButton>
			<IconButton
				label="Go forward"
				disabled={!enabled || !page.forward}
				onClick={() => void client.call("browser.forward", { page: page.id }).catch((error: unknown) => failed("The page could not go forward", error))}
			>
				<ArrowRight className="size-3.5" />
			</IconButton>
			<IconButton
				label="Load the page again"
				disabled={!enabled}
				onClick={() => void client.call("browser.reload", { page: page.id }).catch((error: unknown) => failed("The page could not be loaded again", error))}
			>
				{page.load.state === "loading" ? <LoaderCircle className="size-3.5 animate-spin" /> : <RotateCw className="size-3.5" />}
			</IconButton>
			<AddressBar
				page={page}
				disabled={!enabled}
				onGo={(url) => void client.call("browser.navigate", { page: page.id, url }).catch((error: unknown) => failed("The page could not go there", error))}
			/>
		</>
	);
}

const PAGE_ZOOM = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200] as const;

function BrowserOptions(props: {
	readonly thread: string;
	readonly page: PageListed;
	readonly native: boolean;
	readonly connected: boolean;
	readonly editable: boolean;
	readonly onFind: (() => void) | undefined;
	readonly onZoom: (percent: number) => void;
}): ReactNode {
	const { page, native, editable, onZoom } = props;
	const external = systemBrowserAddress(page.url, useLocalBrowserNode());
	const inspected = page.dev_tools === "open";
	const larger = PAGE_ZOOM.find((value) => value > page.zoom);
	const smaller = PAGE_ZOOM.findLast((value) => value < page.zoom);
	const [opening, setOpening] = useState(false);
	const inspect = (): void => {
		const desktop = window.octoiiBrowser;
		if (desktop === undefined) return;
		setOpening(true);
		void desktop
			.devTools(page.id, inspected ? "close" : "open")
			.catch((error: unknown) => failed("Developer Tools could not be changed", error))
			.finally(() => setOpening(false));
	};
	return (
		<Menu>
			<MenuTrigger render={<IconButton label="Browser options" />}>
				<Ellipsis className="size-3.5" />
			</MenuTrigger>
			<MenuPopup align="end" className="w-48">
				{native ? (
					<MenuItem disabled={props.onFind === undefined} onClick={props.onFind} aria-keyshortcuts="Meta+F Control+F">
						<Search />
						<MenuItemLabel>Find in page</MenuItemLabel>
						<MenuShortcut>{shortcutLabel(FIND_SHORTCUT)}</MenuShortcut>
					</MenuItem>
				) : null}
				{external ? (
					<MenuItem onClick={() => window.open(external.href, "_blank", "noopener,noreferrer")}>
						<ExternalLink />
						<MenuItemLabel>Open in system browser</MenuItemLabel>
					</MenuItem>
				) : null}
				{props.connected && page.downloads.length > 0 ? <BrowserDownloads page={page.id} thread={props.thread} downloads={page.downloads} /> : null}
				<MenuItem
					disabled={!editable || larger === undefined}
					onClick={() => {
						if (larger !== undefined) onZoom(larger);
					}}
				>
					<ZoomIn />
					<MenuItemLabel>Zoom in</MenuItemLabel>
				</MenuItem>
				<MenuItem
					disabled={!editable || smaller === undefined}
					onClick={() => {
						if (smaller !== undefined) onZoom(smaller);
					}}
				>
					<ZoomOut />
					<MenuItemLabel>Zoom out</MenuItemLabel>
				</MenuItem>
				<MenuItem disabled={!editable || page.zoom === 100} onClick={() => onZoom(100)}>
					<MenuItemLabel>Reset zoom · {page.zoom}%</MenuItemLabel>
				</MenuItem>
				{native ? (
					<>
						<MenuSeparator />
						<MenuItem disabled={opening || (!inspected && !editable)} onClick={inspect}>
							<Braces />
							<MenuItemLabel>{inspected ? "Close DevTools" : "Open DevTools"}</MenuItemLabel>
						</MenuItem>
					</>
				) : null}
			</MenuPopup>
		</Menu>
	);
}

/** The words a dialog's buttons and its question take, by its kind: one asking to leave the page names its own. */
function buttonsOf(dialog: PageDialog): { readonly accept: string; readonly dismiss: string | undefined } {
	switch (dialog.kind) {
		case "alert":
			return { accept: "OK", dismiss: undefined };
		case "beforeunload":
			return { accept: "Leave", dismiss: "Stay" };
		default:
			return { accept: "OK", dismiss: "Cancel" };
	}
}

/**
 * The dialog that holds the page, which holds its scripts till it is answered: its message, the text a prompt offers
 * to change, and its buttons — an alert's OK alone.
 */
function DialogBar(props: { readonly dialog: PageDialog; readonly disabled: boolean; readonly onAnswer: (accept: boolean, text?: string) => void }): ReactNode {
	const { dialog } = props;
	const [text, setText] = useState(dialog.offered);
	const words = buttonsOf(dialog);
	const prompt = dialog.kind === "prompt";
	return (
		<form
			role="alertdialog"
			aria-label="The page asks"
			onSubmit={(event) => {
				event.preventDefault();
				if (!props.disabled) props.onAnswer(true, prompt ? text : undefined);
			}}
			className="flex shrink-0 flex-col gap-2 border-b border-border bg-accent px-3 py-2 text-ui"
		>
			<div className="flex items-start gap-2">
				<MessageSquareWarning className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
				<span className="min-w-0 flex-1 break-words whitespace-pre-wrap">
					{dialog.kind === "beforeunload" ? "Leave this page? What it holds may be lost." : dialog.message}
				</span>
			</div>
			{prompt ? (
				<input
					aria-label="The page's question"
					value={text}
					autoFocus
					disabled={props.disabled}
					onChange={(event) => setText(event.target.value)}
					className="h-control-sm rounded-md border border-border bg-background px-2 text-ui outline-none focus:border-ring"
				/>
			) : null}
			<div className="flex justify-end gap-2">
				{words.dismiss === undefined ? null : (
					<Button variant="outline" size="xs" disabled={props.disabled} onClick={() => props.onAnswer(false)}>
						{words.dismiss}
					</Button>
				)}
				<Button type="submit" size="xs" disabled={props.disabled}>
					{words.accept}
				</Button>
			</div>
		</form>
	);
}

/**
 * The page's address: what the person types while they edit it, where the page is otherwise; Enter goes where it says,
 * Escape puts back where the page is. Focused as a blank page opens.
 */
function AddressBar(props: { readonly page: PageListed; readonly disabled: boolean; readonly onGo: (url: string) => void }): ReactNode {
	const shown = shownAddress(props.page.url);
	const [typed, setTyped] = useState<string | undefined>(undefined);
	const go = (event: FormEvent<HTMLFormElement>): void => {
		event.preventDefault();
		const value = typed ?? shown;
		if (value.trim() === "") return;
		props.onGo(addressOf(value));
		setTyped(undefined);
		const field = event.currentTarget.elements.namedItem("address");
		if (field instanceof HTMLInputElement) field.blur();
	};
	return (
		<form onSubmit={go} className="ml-1 flex h-control-sm min-w-0 flex-1 items-center gap-1.5 rounded-md border border-border px-2 focus-within:border-ring">
			<Globe className="size-3.5 shrink-0 text-muted-foreground" />
			<input
				name="address"
				disabled={props.disabled}
				aria-label="Address"
				autoFocus={shown === ""}
				value={typed ?? shown}
				placeholder="Search or type an address"
				spellCheck={false}
				autoCapitalize="off"
				onChange={(event) => setTyped(event.target.value)}
				onFocus={(event) => event.target.select()}
				onBlur={() => setTyped(undefined)}
				onKeyDown={(event) => {
					if (event.key !== "Escape" || event.nativeEvent.isComposing) return;
					setTyped(undefined);
					event.currentTarget.blur();
				}}
				className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-faint-foreground"
			/>
		</form>
	);
}
