export interface OidcConfiguration {
	authority: string
	client_id: string
	redirect_uri: string
	scope: string
	silent_redirect_uri?: string
	monitor_session?: boolean
	automaticSilentRenew?: boolean
	onSigninCallback?: () => void
}

declare global {
	interface Window {
		apiUrl: string
		issuerUrl?: string
		oidcConfiguration?: OidcConfiguration
		inDevelopmentMode: boolean
	}
}

interface RawConfig {
	api_url?: string
	issuer_url?: string
	client_id?: string
	scope?: string
}

let loadingPromise: Promise<void> | null = null

export function loadRuntimeConfig(): Promise<void> {
	if (typeof window === 'undefined') return Promise.resolve()
	if (loadingPromise) return loadingPromise

	loadingPromise = (async () => {
		const isDev = import.meta.env.DEV
		window.inDevelopmentMode = isDev

		let apiUrl: string | undefined
		let issuerUrl: string | undefined
		let clientId: string | undefined
		let scope: string | undefined

		if (isDev) {
			apiUrl = import.meta.env.VITE_API_URL as string | undefined
			issuerUrl = import.meta.env.VITE_OIDC_AUTHORITY as string | undefined
			clientId = import.meta.env.VITE_OIDC_CLIENT_ID as string | undefined
			scope = import.meta.env.VITE_OIDC_SCOPE as string | undefined
		} else {
			try {
				const res = await fetch('/config.json', { cache: 'no-store' })
				if (res.ok) {
					const data: RawConfig = await res.json()
					apiUrl = clean(data.api_url)
					issuerUrl = clean(data.issuer_url)
					clientId = clean(data.client_id)
					scope = clean(data.scope)
				}
			} catch (err) {
				console.error('Failed to load /config.json', err)
			}
		}

		window.apiUrl = apiUrl ?? ''
		window.issuerUrl = issuerUrl

		const redirectUri =
			(import.meta.env.VITE_OIDC_REDIRECT_URI as string | undefined) ??
			`${window.location.origin}/`

		if (issuerUrl && clientId) {
			window.oidcConfiguration = {
				authority: issuerUrl,
				client_id: clientId,
				redirect_uri: redirectUri,
				scope: scope ?? 'openid profile email',
				// Renewal relies on the refresh token rather than a hidden iframe:
				// a silent-redirect page would need its own bundle entry, and one
				// that does not exist is why the previous setting did nothing.
				// What matters is that expiry is acted on - see AuthTokenSync.
				automaticSilentRenew: true,
			}
		} else {
			window.oidcConfiguration = undefined
		}
	})()

	return loadingPromise
}

/// An unsubstituted `${PLACEHOLDER}` means the operator did not supply the
/// value; treating it as configuration would produce a broken request later.
function clean(value: string | undefined): string | undefined {
	if (!value) return undefined
	return value.startsWith('${') && value.endsWith('}') ? undefined : value
}

export function getOidcConfiguration(): OidcConfiguration | undefined {
	if (typeof window === 'undefined') return undefined
	return window.oidcConfiguration
}

export function isDevelopmentMode(): boolean {
	if (typeof window === 'undefined') return import.meta.env.DEV
	return window.inDevelopmentMode
}
