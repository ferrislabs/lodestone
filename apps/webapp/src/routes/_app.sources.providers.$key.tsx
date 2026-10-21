import { createFileRoute } from '@tanstack/react-router'

import { ProviderFeature } from '#/pages/source-providers/feature/provider-feature'
import { providerSearchSchema } from '#/pages/source-providers/lib/search'

export const Route = createFileRoute('/_app/sources/providers/$key')({
	validateSearch: providerSearchSchema,
	head: () => ({ meta: [{ title: 'Source · Lodestone' }] }),
	component: ProviderFeature,
})
