import { cn } from '#/lib/utils'
import {
	MATRIX_CELL_LABELS,
	MATRIX_ROWS,
	type MatrixCell,
	type MatrixColumn,
} from '#/pages/profiles/lib/matrix'

const CELL_TONES: Record<MatrixCell, string> = {
	native: 'font-medium text-emerald-700 dark:text-emerald-400',
	after: 'text-amber-700 dark:text-amber-400',
	none: 'text-muted-foreground',
}

interface CriteriaMatrixProps {
	columns: readonly MatrixColumn[]
}

export function CriteriaMatrix({ columns }: CriteriaMatrixProps) {
	return (
		<section
			aria-labelledby="criteria-matrix-title"
			className="flex flex-col gap-3 rounded-xl border bg-card p-5"
		>
			<h2 id="criteria-matrix-title" className="text-base font-semibold">
				Ce que chaque fournisseur applique
			</h2>
			<p className="text-xs text-muted-foreground">
				Natif : filtré par l'API du fournisseur. Après collecte : filtré par
				Lodestone sur chaque annonce collectée.
			</p>
			{columns.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					Choisir un fournisseur pour voir ce qu'il applique.
				</p>
			) : (
				<div className="overflow-x-auto">
					<table className="w-full border-collapse text-sm">
						<thead>
							<tr className="border-b text-left text-xs text-muted-foreground">
								<th scope="col" className="px-3 py-2 font-medium">
									Critère
								</th>
								{columns.map((column) => (
									<th
										key={column.key}
										scope="col"
										className="px-3 py-2 font-medium"
									>
										{column.name}
									</th>
								))}
							</tr>
						</thead>
						<tbody>
							{MATRIX_ROWS.map((row) => (
								<tr key={row.criterion} className="border-b last:border-b-0">
									<th scope="row" className="px-3 py-2 text-left font-normal">
										{row.label}
									</th>
									{columns.map((column) => {
										const cell = column.cells[row.criterion]
										return (
											<td
												key={column.key}
												data-cell={`${row.criterion}:${column.key}`}
												className={cn('px-3 py-2', CELL_TONES[cell])}
											>
												{MATRIX_CELL_LABELS[cell]}
											</td>
										)
									})}
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</section>
	)
}
