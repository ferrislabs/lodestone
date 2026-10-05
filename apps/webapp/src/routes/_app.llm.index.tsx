import { createFileRoute } from '@tanstack/react-router'

import { LlmOverviewFeature } from '#/pages/llm/feature/llm-overview-feature'

export const Route = createFileRoute('/_app/llm/')({
	component: LlmOverviewFeature,
})
