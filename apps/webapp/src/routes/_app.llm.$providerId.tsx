import { createFileRoute } from '@tanstack/react-router'

import { ProviderEditFeature } from '#/pages/llm/feature/provider-form-feature'

export const Route = createFileRoute('/_app/llm/$providerId')({
	head: () => ({ meta: [{ title: 'Fournisseur LLM · Lodestone' }] }),
	component: ProviderEditFeature,
})
