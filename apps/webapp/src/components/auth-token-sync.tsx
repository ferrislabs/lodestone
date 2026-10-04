import { useEffect } from 'react'
import { useAuth } from 'react-oidc-context'

import { clearAuth, setAuth } from '#/store/auth.store'

export function AuthTokenSync() {
	const auth = useAuth()

	useEffect(() => {
		if (auth.isAuthenticated && auth.user?.access_token) {
			setAuth({
				accessToken: auth.user.access_token,
				expiresAt: auth.user.expires_at ?? null,
				isAuthenticated: true,
			})
		} else if (!auth.isLoading) {
			clearAuth()
		}
	}, [
		auth.isAuthenticated,
		auth.isLoading,
		auth.user?.access_token,
		auth.user?.expires_at,
	])

	// Expiry was collected and never acted on: when renewal failed the library
	// kept the user in its store, so the interface stayed authenticated-looking
	// and the next call would have carried a stale token. Drop the credential
	// and let the guard send the user back to the provider.
	useEffect(() => {
		const events = auth.events
		if (!events) return

		const onExpired = () => {
			clearAuth()
			void auth.signinRedirect()
		}

		return events.addAccessTokenExpired(onExpired)
	}, [auth.events, auth.signinRedirect])

	return null
}
