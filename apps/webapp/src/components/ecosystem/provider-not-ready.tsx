import { PROVIDER_NOT_READY } from '#/lib/ecosystem'

interface ProviderNotReadyProps {
	href: string
}

export function ProviderNotReady({ href }: ProviderNotReadyProps) {
	return (
		<a
			href={href}
			data-provider-not-ready={href}
			className="inline-flex w-fit items-center rounded-md border border-red-200 bg-red-50 px-2 py-0.5 text-xs font-medium text-red-700 hover:underline"
		>
			{PROVIDER_NOT_READY}
		</a>
	)
}
