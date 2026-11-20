/**
 * A page of the browser drawn in an element, as a VNC viewer draws a framebuffer: followed in a view the size of the
 * element's room once it has one, told again as the room settles at another; each frame the node sends drawn on a
 * canvas at the page's CSS size, the newest alone where frames come faster than they are decoded. The person's mouse
 * over it and their keys while it is focused are sent to the page — a drag held to it past its edge, a move sent once
 * a frame, a key the app answers as its shortcut kept from it, as a window's menu takes its shortcuts before the view
 * focused does — and the text they paste, compose with an input method, or put in by no key of its own, sent as
 * text, through a field of its own that takes the focus as the page is shown, where the person types in no field, as
 * a terminal's hidden field does.
 */
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { PageFrame, PageListed, PageMouse, PageView, PickElement, View as PageSize } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { type DrawnPage, editingOf, modifiersOf, PageInputGate, type PageMode, pickViewOf, typedBy } from "../../core/browser.ts";
import { isTypedIn } from "./shortcut.ts";

/** How long the room stands at a size before the page is laid out at it. */
const SETTLE_MS = 120;

/** The pixels a wheel's line turns, where it counts in lines. */
const LINE_PIXELS = 16;

/** A local selection, with the viewport actually drawn rather than the container's size. */
export type ElementSelection = Pick<PickElement, "x" | "y" | "expected_url" | "view">;
export type PageInteraction = { readonly mode: PageMode; readonly select: (selection: ElementSelection) => void; readonly cancel: () => void };
type Options = { readonly page: PageListed; readonly interaction: PageInteraction };
type Input = { readonly update: () => void; readonly stop: () => void };

/** The stable ref mounts a page once; changing browse/pick only updates its local input routing. */
export function usePageView(
	page: PageListed,
	interaction: PageInteraction,
): {
	readonly mount: (element: HTMLDivElement | null) => (() => void) | undefined;
	readonly ready: boolean;
} {
	const client = useClient();
	const id = page.id;
	const latest = useRef<Options>({ page, interaction });
	const input = useRef<Input | undefined>(undefined);
	const [shown, setShown] = useState<DrawnPage | undefined>(undefined);
	useLayoutEffect(() => {
		latest.current = { page, interaction };
		input.current?.update();
	}, [page, interaction]);
	const mount = useCallback(
		(element: HTMLDivElement | null) => {
			if (element === null) return undefined;
			setShown(undefined);
			const surface = mountSurface(element, setShown);
			let view: PageView | undefined;
			let settling: ReturnType<typeof setTimeout> | undefined;
			const settled = (): void => {
				const size = sizeOf(element);
				if (size === undefined) return;
				if (view === undefined) view = client.page(id, size, { frame: surface.draw });
				else view.resize(size);
			};
			const schedule = (): void => {
				clearTimeout(settling);
				settling = setTimeout(settled, view === undefined ? 0 : SETTLE_MS);
			};
			const observer = new ResizeObserver(schedule);
			observer.observe(element, { box: "device-pixel-content-box" });
			const routing = takeInput(
				element,
				surface,
				(send) => view !== undefined && send(view),
				() => latest.current,
			);
			input.current = routing;
			routing.update();
			// A page opened takes the focus from anything else, never from a field the person types in.
			if (!isTypedIn(document.activeElement)) surface.field.focus({ preventScroll: true });
			return () => {
				clearTimeout(settling);
				observer.disconnect();
				routing.stop();
				if (input.current === routing) input.current = undefined;
				view?.stop();
				surface.remove();
			};
		},
		[client, id],
	);
	return { mount, ready: pickViewOf(shown, page.view) !== undefined };
}

/** The size of `element`'s room as a page's view, none while it has no room. */
function sizeOf(element: HTMLElement): PageSize | undefined {
	const width = Math.floor(element.clientWidth);
	const height = Math.floor(element.clientHeight);
	if (width === 0 || height === 0) return undefined;
	return { width, height, scale: window.devicePixelRatio };
}

/** What a page is drawn on: its canvas, the field that takes its keys, drawing a frame, and taking them out again. */
type Surface = {
	readonly canvas: HTMLCanvasElement;
	readonly field: HTMLTextAreaElement;
	readonly shown: DrawnPage | undefined;
	/** Where a point of the page's CSS pixels is, from a point of the window's. */
	pointOf(clientX: number, clientY: number): { readonly x: number; readonly y: number };
	draw(frame: PageFrame): void;
	remove(): void;
};

/** A canvas and a field put in `element`: the canvas blank over all of it till a frame comes, then each frame drawn on it, the
 * newest where they come faster than they are decoded. */
function mountSurface(element: HTMLElement, onShown: (size: DrawnPage) => void): Surface {
	const canvas = document.createElement("canvas");
	// The room filled, as a blank page fills it, till the first frame gives the page's own size.
	canvas.className = "absolute top-0 left-0 size-full bg-white";
	const field = document.createElement("textarea");
	field.className = "absolute top-0 left-0 size-px resize-none opacity-0";
	field.setAttribute("aria-label", "The page");
	field.autocapitalize = "off";
	field.spellcheck = false;
	element.append(canvas, field);
	const context = canvas.getContext("2d");
	let next: PageFrame | undefined;
	let drawing = false;
	let disposed = false;
	let shown: DrawnPage | undefined;
	const drawNext = async (): Promise<void> => {
		drawing = true;
		for (let frame = next; frame !== undefined; frame = next) {
			next = undefined;
			const image = await createImageBitmap(new Blob([new Uint8Array(frame.jpeg)], { type: "image/jpeg" })).catch(() => undefined);
			if (image === undefined) continue;
			if (disposed) {
				image.close();
				return;
			}
			const rasterChanged = canvas.width !== image.width || canvas.height !== image.height;
			if (rasterChanged) Object.assign(canvas, { width: image.width, height: image.height });
			const resized = rasterChanged || shown?.width !== frame.width || shown.height !== frame.height;
			if (resized) {
				Object.assign(canvas.style, { width: `${String(frame.width)}px`, height: `${String(frame.height)}px` });
			}
			context?.drawImage(image, 0, 0);
			if (resized) {
				shown = { width: frame.width, height: frame.height, pixelWidth: image.width, pixelHeight: image.height };
				onShown(shown);
			}
			image.close();
		}
		drawing = false;
	};
	return {
		canvas,
		field,
		get shown() {
			return shown;
		},
		pointOf: (clientX, clientY) => {
			const box = canvas.getBoundingClientRect();
			const across = box.width === 0 ? 1 : (shown?.width ?? 0) / box.width;
			const down = box.height === 0 ? 1 : (shown?.height ?? 0) / box.height;
			return { x: (clientX - box.left) * across, y: (clientY - box.top) * down };
		},
		draw: (frame) => {
			next = frame;
			if (!drawing) void drawNext();
		},
		remove: () => {
			disposed = true;
			next = undefined;
			canvas.remove();
			field.remove();
		},
	};
}

/** A mouse's button, as the browser's protocol names it. */
type Button = NonNullable<PageMouse["button"]>;

/** The DOM's buttons, by their number, as the browser's protocol names them. */
const BUTTONS: readonly Button[] = ["left", "middle", "right"];

/**
 * Sends the person's mouse over `element` and their keys in its field to the page, through `send`, which reaches the
 * page's view where it is followed; what stops it.
 */
type InputScope = {
	readonly element: HTMLElement;
	readonly surface: Surface;
	readonly gate: PageInputGate;
	readonly send: (sending: (view: PageView) => void) => void;
	readonly options: () => Options;
	readonly on: AddEventListenerOptions;
};

function takeInput(element: HTMLElement, surface: Surface, send: InputScope["send"], options: () => Options): Input {
	const listening = new AbortController();
	const gate = new PageInputGate();
	const scope: InputScope = { element, surface, send, options, gate, on: { signal: listening.signal, passive: false } };
	const cancelMove = takeMouse(scope);
	takeKeys(scope, cancelMove);
	const updateText = takeText(scope);
	return {
		update: () => {
			const mode = options().interaction.mode;
			if (gate.mode === mode) return;
			cancelMove();
			gate.setMode(mode);
			updateText();
		},
		stop: () => {
			cancelMove();
			listening.abort();
		},
	};
}

/** Prevents a local selection gesture from reaching other page or application handlers. */
function block(event: Event): void {
	event.preventDefault();
	event.stopPropagation();
}

/** Mouse input is forwarded only in browse mode; a local pick finishes after the matching release. */
function takeMouse({ element, surface, gate, send, options, on }: InputScope): () => void {
	const mouse = (event: MouseEvent, motion: PageMouse["motion"], button: Button, clicks: number): void => {
		const point = surface.pointOf(event.clientX, event.clientY);
		send((view) => view.mouse({ motion, ...point, button, buttons: event.buttons, clicks, modifiers: modifiersOf(event), delta_x: 0, delta_y: 0 }));
	};
	let moved: MouseEvent | undefined;
	let frame = 0;
	const cancelMove = (): void => {
		cancelAnimationFrame(frame);
		frame = 0;
		moved = undefined;
	};
	element.addEventListener("pointerdown", (event) => element.setPointerCapture(event.pointerId), on);
	element.addEventListener(
		"mousedown",
		(event) => {
			event.preventDefault();
			surface.field.focus({ preventScroll: true });
			if (gate.mouseDown(event.button) === "block") {
				block(event);
				return;
			}
			mouse(event, "down", BUTTONS[event.button] ?? "none", event.detail);
		},
		on,
	);
	element.addEventListener(
		"mouseup",
		(event) => {
			const action = gate.mouseUp(event.button);
			if (action === "forward") {
				mouse(event, "up", BUTTONS[event.button] ?? "none", event.detail);
				return;
			}
			block(event);
			if (action !== "select") return;
			const { page, interaction } = options();
			const view = pickViewOf(surface.shown, page.view);
			const point = surface.pointOf(event.clientX, event.clientY);
			if (
				view === undefined ||
				!Number.isFinite(point.x) ||
				!Number.isFinite(point.y) ||
				point.x < 0 ||
				point.y < 0 ||
				point.x >= view.width ||
				point.y >= view.height
			)
				return;
			interaction.select({ ...point, view, expected_url: page.url });
		},
		on,
	);
	element.addEventListener(
		"mousemove",
		(event) => {
			if (!gate.forwardsPointer()) {
				cancelMove();
				block(event);
				return;
			}
			moved = event;
			frame ||= requestAnimationFrame(() => {
				frame = 0;
				if (moved !== undefined && gate.forwardsPointer()) mouse(moved, "move", heldOf(moved.buttons), 0);
			});
		},
		on,
	);
	element.addEventListener(
		"wheel",
		(event) => {
			event.preventDefault();
			if (!gate.forwardsPointer()) {
				block(event);
				return;
			}
			const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? LINE_PIXELS : 1;
			const point = surface.pointOf(event.clientX, event.clientY);
			send((view) =>
				view.mouse({
					motion: "wheel",
					...point,
					button: "none",
					buttons: event.buttons,
					clicks: 0,
					modifiers: modifiersOf(event),
					delta_x: event.deltaX * unit,
					delta_y: event.deltaY * unit,
				}),
			);
		},
		on,
	);
	element.addEventListener("contextmenu", (event) => event.preventDefault(), on);
	return cancelMove;
}

/** Selection Escape is heard before app shortcuts; every swallowed key's release remains local. */
function takeKeys({ surface: { field }, gate, send, options, on }: InputScope, cancelMove: () => void): void {
	window.addEventListener(
		"keydown",
		(event) => {
			if (gate.mode === "browse" || (event.target !== field && event.key !== "Escape")) return;
			gate.keyDown(event.code);
			block(event);
			if (event.key === "Escape") {
				cancelMove();
				options().interaction.cancel();
			}
		},
		{ ...on, capture: true },
	);
	// In browse mode the window's shortcuts get the key before the page does.
	window.addEventListener(
		"keydown",
		(event) => {
			if (event.target === field && gate.keyDown(event.code) === "block") {
				block(event);
				return;
			}
			if (event.target !== field || event.defaultPrevented || event.isComposing || event.keyCode === 229 || isPaste(event)) return;
			event.preventDefault();
			const text = event.ctrlKey || event.metaKey ? undefined : typedBy(event.key);
			const commands = editingOf(event);
			const pressed = {
				pressed: true,
				key: event.key,
				code: event.code,
				modifiers: modifiersOf(event),
				key_code: event.keyCode,
				commands: [...(commands ?? [])],
			};
			send((view) => view.key({ ...pressed, ...(text === undefined ? {} : { text }) }));
		},
		on,
	);
	window.addEventListener(
		"keyup",
		(event) => {
			if (event.target !== field) {
				gate.keyUp(event.code);
				return;
			}
			if (gate.keyUp(event.code) === "block") {
				block(event);
				return;
			}
			if (event.isComposing || event.keyCode === 229) return;
			send((view) => view.key({ pressed: false, key: event.key, code: event.code, modifiers: modifiersOf(event), key_code: event.keyCode, commands: [] }));
		},
		on,
	);
}

/** IME, paste and text-only input share the same local selection boundary. */
function takeText({ surface: { field, canvas }, gate, send, on }: InputScope): () => void {
	let localComposition = false;
	let composing = false;
	field.addEventListener(
		"compositionstart",
		(event) => {
			composing = true;
			localComposition = gate.mode !== "browse";
			if (localComposition) block(event);
		},
		on,
	);
	field.addEventListener(
		"beforeinput",
		(event) => {
			if (gate.mode !== "browse" || localComposition) block(event);
		},
		on,
	);
	field.addEventListener(
		"compositionend",
		(event) => {
			composing = false;
			field.value = "";
			if (gate.mode !== "browse" || localComposition) {
				localComposition = false;
				block(event);
				return;
			}
			if (event.data !== "") send((view) => view.text(event.data));
		},
		on,
	);
	field.addEventListener(
		"input",
		(event) => {
			if (gate.mode !== "browse" || localComposition) {
				field.value = "";
				block(event);
				return;
			}
			if (event instanceof InputEvent && event.isComposing) return;
			const text = field.value;
			field.value = "";
			if (text !== "") send((view) => view.text(text));
		},
		on,
	);
	field.addEventListener(
		"paste",
		(event) => {
			event.preventDefault();
			if (gate.mode !== "browse") {
				block(event);
				return;
			}
			const text = event.clipboardData?.getData("text/plain") ?? "";
			if (text !== "") send((view) => view.text(text));
		},
		on,
	);
	return () => {
		if (gate.mode !== "browse" && composing) localComposition = true;
		field.value = "";
		field.readOnly = gate.mode !== "browse";
		field.setAttribute("aria-label", gate.mode === "browse" ? "The page" : "Select an element in the page. Escape cancels.");
		canvas.style.cursor = gate.mode === "browse" ? "" : gate.mode === "pick" ? "crosshair" : "wait";
		if (gate.mode === "pick") field.focus({ preventScroll: true });
	};
}

/** The button a move holds, as the browser's protocol names it: the first of those the DOM counts held, left, middle then right. */
function heldOf(buttons: number): Button {
	if (buttons & 1) return "left";
	if (buttons & 4) return "middle";
	if (buttons & 2) return "right";
	return "none";
}

/** Whether a key pressed pastes: ⌘V, or Ctrl+V off a Mac — left to the field, whose paste is sent as text. */
function isPaste(event: KeyboardEvent): boolean {
	return (event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "v";
}
