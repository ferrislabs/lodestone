import { formatProblem } from '#/pages/profiles/lib/problems'
import type { Problem } from '#/pages/profiles/lib/types'

interface ProfileProblemsProps {
	problems: readonly Problem[]
	title?: string
}

export function ProfileProblems({
	problems,
	title = 'La définition contient des problèmes',
}: ProfileProblemsProps) {
	if (problems.length === 0) return null
	return (
		<div
			role="alert"
			className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm"
		>
			<p className="font-medium text-destructive">{title}</p>
			<ul className="mt-2 flex flex-col gap-1">
				{problems.map((problem) => (
					<li
						key={formatProblem(problem)}
						className="font-mono text-xs text-destructive"
					>
						<span className="font-semibold">{problem.path}</span>
						{': '}
						{problem.message}
					</li>
				))}
			</ul>
		</div>
	)
}
