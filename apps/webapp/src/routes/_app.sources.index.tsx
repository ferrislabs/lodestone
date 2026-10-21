import { createFileRoute } from '@tanstack/react-router'

import { CatalogFeature } from '#/pages/source-providers/feature/catalog-feature'

export const Route = createFileRoute('/_app/sources/')({
	head: () => ({ meta: [{ title: 'Catalogue des sources · Lodestone' }] }),
	component: CatalogFeature,
})
