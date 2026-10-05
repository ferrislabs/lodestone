import { createFileRoute } from '@tanstack/react-router'

import { DraftsListFeature } from '#/pages/drafts/feature/drafts-list-feature'
import { draftsSearchSchema } from '#/pages/drafts/lib/search'

export const Route = createFileRoute('/_app/drafts/')({
	validateSearch: draftsSearchSchema,
	component: DraftsListFeature,
})
