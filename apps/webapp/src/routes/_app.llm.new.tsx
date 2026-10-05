import { createFileRoute } from '@tanstack/react-router'

import { ProviderNewFeature } from '#/pages/llm/feature/provider-form-feature'

export const Route = createFileRoute('/_app/llm/new')({
	component: ProviderNewFeature,
})
