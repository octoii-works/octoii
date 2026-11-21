/** Keeps each Electron guest attached to the document while its browser tab is hidden. */
import { type ReactNode, useSyncExternalStore } from "react";
import { routeOf } from "../../core/route.ts";
import { pageKey, showSurface } from "../../components/state/panel.ts";
import type { FindablePage } from "../../components/effects/browser-find.ts";

type Hosted = {
	readonly page: string;
	readonly thread: string;
	readonly partition: string;
	readonly start: string;
	readonly width: number;
	readonly height: number;
};
type Portal = (children: ReactNode, host: HTMLDivElement) => ReactNode;
export type DesktopPage = {
	readonly host: HTMLDivElement;
	readonly guest: FindablePage;
	readonly park: () => void;
	readonly render: (children: ReactNode) => ReactNode;
};

declare global {
	interface Window {
		readonly octoiiBrowser?: {
			readonly pages: (listener: (pages: readonly Hosted[]) => void) => () => void;
			readonly thread: (thread: string | null, session: string | null) => void;
			readonly devTools: (page: string, action: "open" | "close") => Promise<void>;
		};
	}
}

const guests = new Map<string, DesktopPage>();
const listeners = new Set<() => void>();
let localNode = false;

function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

export function useDesktopPage(page: string): DesktopPage | undefined {
	return useSyncExternalStore(subscribe, () => guests.get(page));
}

/** A snapshot is sent only after the desktop verifies the node for this window. */
export function useLocalBrowserNode(): boolean {
	return useSyncExternalStore(subscribe, () => localNode);
}

function mount(page: Hosted, portal: Portal): DesktopPage {
	const host = document.createElement("div");
	host.className = "pointer-events-none fixed z-30 isolate overflow-hidden bg-white";
	host.dataset["desktopPage"] = page.page;
	// SAFETY: This mount runs only for pages announced by the Electron desktop bridge, which provides the webview API.
	const webview = document.createElement("webview") as FindablePage;
	webview.className = "pointer-events-none absolute inset-0 size-full";
	webview.setAttribute("partition", page.partition);
	webview.setAttribute("allowpopups", "");
	webview.setAttribute("src", page.start);
	host.append(webview);
	const park = (): void => {
		Object.assign(host.style, {
			left: `-${String(page.width + 8)}px`,
			top: "0px",
			width: `${String(page.width)}px`,
			height: `${String(page.height)}px`,
			transform: "none",
			clipPath: "none",
		});
	};
	park();
	document.body.append(host);
	return { host, guest: webview, park, render: (children) => portal(children, host) };
}

/** Starts the desktop's native pages before the workspace asks for their display slots. */
export function startDesktopPages(portal: Portal): void {
	window.octoiiBrowser?.pages((pages) => {
		localNode = true;
		const kept = new Set(pages.map((page) => page.page));
		for (const [id, guest] of guests) {
			if (kept.has(id)) continue;
			guest.host.remove();
			guests.delete(id);
		}
		for (const page of pages) {
			if (guests.has(page.page)) continue;
			guests.set(page.page, mount(page, portal));
			const route = routeOf(window.location.hash);
			if (route.place === "thread" && route.thread === page.thread) showSurface(page.thread, pageKey(page.page));
		}
		for (const listener of listeners) listener();
	});
}

/** Shows the guest over its slot, moving it off screen while the slot is out of the workspace. */
export function placeDesktopPage(
	page: DesktopPage,
	slot: HTMLElement,
	scaling: { readonly across: number; readonly down: number; readonly transform: string },
): () => void {
	let frame = 0;
	const room = slot.closest("[data-browser-viewport]");
	const place = (): void => {
		const rect = slot.getBoundingClientRect();
		const bounds = room?.getBoundingClientRect();
		const clip = room && bounds ? { top: bounds.top, left: bounds.left, right: bounds.left + room.clientWidth, bottom: bounds.top + room.clientHeight } : rect;
		if (rect.width === 0 || rect.height === 0 || rect.bottom <= clip.top || rect.top >= clip.bottom || rect.right <= clip.left || rect.left >= clip.right) {
			page.park();
			return;
		}
		const edges = [
			(clip.top - rect.top) / scaling.down,
			(rect.right - clip.right) / scaling.across,
			(rect.bottom - clip.bottom) / scaling.down,
			(clip.left - rect.left) / scaling.across,
		]
			.map((edge) => `${String(Math.max(0, edge))}px`)
			.join(" ");
		Object.assign(page.host.style, {
			left: `${String(rect.left)}px`,
			top: `${String(rect.top)}px`,
			width: `${String(slot.clientWidth)}px`,
			height: `${String(slot.clientHeight)}px`,
			transformOrigin: "top left",
			transform: scaling.transform,
			clipPath: `inset(${edges})`,
		});
	};
	const schedule = (): void => {
		frame ||= requestAnimationFrame(() => {
			frame = 0;
			place();
		});
	};
	const observer = new ResizeObserver(schedule);
	observer.observe(slot);
	if (room) observer.observe(room);
	room?.addEventListener("scroll", schedule, { passive: true });
	window.addEventListener("resize", schedule);
	place();
	return () => {
		cancelAnimationFrame(frame);
		observer.disconnect();
		room?.removeEventListener("scroll", schedule);
		window.removeEventListener("resize", schedule);
		page.park();
	};
}
