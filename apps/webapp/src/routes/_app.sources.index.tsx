import { createFileRoute } from '@tanstack/react-router'

import { CatalogFeature } from '#/pages/source-providers/feature/catalog-feature'
import { catalogSearchSchema } from '#/pages/source-providers/lib/search'

export const Route = createFileRoute('/_app/sources/')({
	validateSearch: catalogSearchSchema,
	head: () => ({ meta: [{ title: 'Catalogue des sources · Lodestone' }] }),
	component: CatalogFeature,
})
