import {
	CURVE_SIZE,
	curveLine,
	curvePoints,
} from '#/pages/quality/lib/calibration'
import type { ReliabilityBin } from '#/pages/quality/lib/types'

interface ReliabilityCurveProps {
	label: string
	bins: readonly ReliabilityBin[]
}

const PADDING = 6
const VIEW_BOX = `${-PADDING} ${-PADDING} ${CURVE_SIZE + 2 * PADDING} ${CURVE_SIZE + 2 * PADDING}`

export function ReliabilityCurve({ label, bins }: ReliabilityCurveProps) {
	const points = curvePoints(bins)
	return (
		<svg
			role="img"
			aria-label={label}
			viewBox={VIEW_BOX}
			className="h-28 w-28 shrink-0 text-foreground"
		>
			<title>{label}</title>
			<rect
				x={0}
				y={0}
				width={CURVE_SIZE}
				height={CURVE_SIZE}
				fill="none"
				stroke="currentColor"
				strokeOpacity={0.2}
			/>
			<line
				data-part="diagonal"
				x1={0}
				y1={CURVE_SIZE}
				x2={CURVE_SIZE}
				y2={0}
				stroke="currentColor"
				strokeOpacity={0.4}
				strokeDasharray="4 3"
			/>
			{points.length > 1 ? (
				<polyline
					data-part="curve"
					points={curveLine(points)}
					fill="none"
					stroke="currentColor"
					strokeWidth={1.5}
				/>
			) : null}
			{points.map((point) => (
				<circle
					key={`${point.x}-${point.y}`}
					data-part="bin"
					cx={point.x}
					cy={point.y}
					r={2.5}
					fill="currentColor"
				/>
			))}
		</svg>
	)
}
