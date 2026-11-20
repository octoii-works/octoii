/** Page dimensions are CSS pixels. Preview zoom only scales how those pixels are shown. */
export type ViewportSize = { readonly width: number; readonly height: number };
export type ViewportAxis = keyof ViewportSize;
export type ViewportZoom = "fit" | "50" | "75" | "100" | "125" | "150";

export const MAX_VIEWPORT = 4096;
export const VIEWPORT_MARGIN = 12;

export const VIEWPORT_PRESETS = [
	{ value: "phone", label: "Phone", width: 390, height: 844 },
	{ value: "tablet", label: "Tablet", width: 768, height: 1024 },
	{ value: "desktop", label: "Desktop · 16∶9", width: 1280, height: 720 },
	{ value: "standard", label: "Standard · 4∶3", width: 1024, height: 768 },
	{ value: "square", label: "Square · 1∶1", width: 800, height: 800 },
] as const;

export function viewportScale(size: ViewportSize, room: ViewportSize | undefined, zoom: ViewportZoom): number {
	if (zoom !== "fit") return Number(zoom) / 100;
	if (room === undefined) return 1;
	return Math.min(1, Math.max(1, room.width - 2 * VIEWPORT_MARGIN) / size.width, Math.max(1, room.height - 2 * VIEWPORT_MARGIN) / size.height);
}

export function editedViewport(size: ViewportSize, axis: ViewportAxis, value: number, ratio?: number): ViewportSize | undefined {
	const next =
		axis === "width"
			? { width: value, height: ratio === undefined ? size.height : Math.round(value / ratio) }
			: { width: ratio === undefined ? size.width : Math.round(value * ratio), height: value };
	return [next.width, next.height].every((dimension) => Number.isInteger(dimension) && dimension >= 1 && dimension <= MAX_VIEWPORT) ? next : undefined;
}
