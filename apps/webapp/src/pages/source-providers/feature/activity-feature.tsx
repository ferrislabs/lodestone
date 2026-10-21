import { useNavigate } from '@tanstack/react-router'
import { SourcesTabs } from '#/pages/source-providers/ui/sources-tabs'
import { SourcesFeature } from '#/pages/sources/feature/sources-feature'

export function SourcesActivityFeature() {
	const navigate = useNavigate()
	return (
		<>
			<div className="px-4 pt-4 md:px-8 md:pt-8">
				<SourcesTabs
					active="activity"
					onSelect={(tab) => {
						if (tab === 'catalog') void navigate({ to: '/sources' })
					}}
				/>
			</div>
			<SourcesFeature />
		</>
	)
}
