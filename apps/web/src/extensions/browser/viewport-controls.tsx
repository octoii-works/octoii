import { type ReactNode, useState } from "react";
import { LockKeyhole, LockKeyholeOpen, Repeat2 } from "lucide-react";
import { MAX_VIEWPORT, VIEWPORT_PRESETS, editedViewport, type ViewportAxis, type ViewportSize, type ViewportZoom } from "../../core/browser-viewport.ts";
import { IconButton } from "../../components/ui/button.tsx";
import { Input } from "../../components/ui/input.tsx";
import { Select, type SelectItem } from "../../components/ui/select.tsx";

const ZOOMS: readonly SelectItem<ViewportZoom>[] = [
	{ value: "50", label: "50%" },
	{ value: "75", label: "75%" },
	{ value: "100", label: "100%" },
	{ value: "125", label: "125%" },
	{ value: "150", label: "150%" },
];

const PRESETS = [{ value: "custom", label: "Custom" }, ...VIEWPORT_PRESETS] as const;

export function ViewportControls(props: {
	readonly size: ViewportSize;
	readonly editable: boolean;
	readonly saving: boolean;
	readonly zoom: ViewportZoom;
	readonly scale: number;
	readonly onSize: (size: ViewportSize) => void;
	readonly onZoom: (zoom: ViewportZoom) => void;
}): ReactNode {
	const { size, editable, saving, zoom, scale, onSize, onZoom } = props;
	const canEdit = editable && !saving;
	const [ratio, setRatio] = useState<number>();
	const locked = ratio !== undefined;
	const preset = VIEWPORT_PRESETS.find(
		(each) => (each.width === size.width && each.height === size.height) || (each.height === size.width && each.width === size.height),
	);
	const edit = (axis: ViewportAxis, value: number): void => {
		const next = editedViewport(size, axis, value, ratio);
		if (next !== undefined) onSize(next);
	};
	const choose = (next: ViewportSize): void => {
		if (locked) setRatio(next.width / next.height);
		onSize(next);
	};
	return (
		<div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border bg-muted px-2 py-1 text-xs" role="group" aria-label="Responsive viewport">
			{editable ? (
				<Select
					label="Viewport preset"
					value={preset?.value ?? "custom"}
					items={PRESETS}
					disabled={saving}
					className="h-control-sm min-w-26 bg-background text-xs"
					onValueChange={(value) => {
						const chosen = VIEWPORT_PRESETS.find((each) => each.value === value);
						if (chosen) choose(chosen);
					}}
				/>
			) : null}
			<div className="flex items-center gap-1 tabular-nums">
				<Dimension label="Viewport width" value={size.width} editable={canEdit} onValue={(value) => edit("width", value)} />
				<span aria-hidden className="text-muted-foreground">
					×
				</span>
				<Dimension label="Viewport height" value={size.height} editable={canEdit} onValue={(value) => edit("height", value)} />
			</div>
			{editable ? (
				<div className="flex items-center gap-0.5">
					<IconButton
						label={locked ? "Unlock aspect ratio" : "Lock aspect ratio"}
						aria-pressed={locked}
						disabled={saving}
						onClick={() => setRatio(locked ? undefined : size.width / size.height)}
					>
						{locked ? <LockKeyhole className="size-3.5" /> : <LockKeyholeOpen className="size-3.5" />}
					</IconButton>
					<IconButton label="Rotate viewport" disabled={saving} onClick={() => choose({ width: size.height, height: size.width })}>
						<Repeat2 className="size-3.5" />
					</IconButton>
				</div>
			) : null}
			<Select
				label="Preview zoom"
				value={zoom}
				items={[{ value: "fit", label: `Fit (${String(Math.round(scale * 100))}%)` }, ...ZOOMS]}
				className="ml-auto h-control-sm min-w-24 bg-background text-xs"
				onValueChange={onZoom}
			/>
		</div>
	);
}

function Dimension(props: {
	readonly label: string;
	readonly value: number;
	readonly editable: boolean;
	readonly onValue: (value: number) => void;
}): ReactNode {
	const [typed, setTyped] = useState<string>();
	return (
		<Input
			size="sm"
			className="w-16 shrink-0 bg-background px-1.5 text-center text-xs [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none"
			aria-label={props.label}
			inputMode="numeric"
			type="number"
			min={1}
			max={MAX_VIEWPORT}
			value={typed ?? String(props.value)}
			readOnly={!props.editable}
			onChange={(event) => setTyped(event.currentTarget.value)}
			onBlur={(event) => {
				const value = event.currentTarget.value;
				if (props.editable && value !== "" && Number(value) !== props.value) props.onValue(Number(value));
				setTyped(undefined);
			}}
			onKeyDown={(event) => {
				if (event.key === "Enter") event.currentTarget.blur();
				else if (event.key === "Escape") {
					event.stopPropagation();
					setTyped(undefined);
				}
			}}
		/>
	);
}
