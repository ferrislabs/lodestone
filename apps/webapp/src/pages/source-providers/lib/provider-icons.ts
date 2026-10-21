import {
	Briefcase,
	Contact,
	Database,
	type LucideIcon,
	Network,
	Radar,
	Scale,
	ShieldCheck,
} from 'lucide-react'

import boamp from '#/assets/providers/boamp.png'
import companyResearch from '#/assets/providers/company_research.svg'
import decp from '#/assets/providers/decp.svg'
import franceTravail from '#/assets/providers/france_travail.svg'
import hackerNewsHiring from '#/assets/providers/hacker_news_hiring.svg'
import ted from '#/assets/providers/ted.png'
import welcomeToTheJungle from '#/assets/providers/welcome_to_the_jungle.png'

export type ProviderIcon =
	| { kind: 'logo'; src: string }
	| { kind: 'lucide'; Icon: LucideIcon }

const ICONS: Readonly<Record<string, ProviderIcon>> = {
	boamp: { kind: 'logo', src: boamp },
	decp: { kind: 'logo', src: decp },
	ted: { kind: 'logo', src: ted },
	france_travail: { kind: 'logo', src: franceTravail },
	ats: { kind: 'lucide', Icon: Briefcase },
	hacker_news_hiring: { kind: 'logo', src: hackerNewsHiring },
	welcome_to_the_jungle: { kind: 'logo', src: welcomeToTheJungle },
	certificate_transparency: { kind: 'lucide', Icon: ShieldCheck },
	dns_cname: { kind: 'lucide', Icon: Network },
	http_probe: { kind: 'lucide', Icon: Radar },
	contacts: { kind: 'lucide', Icon: Contact },
	company_research: { kind: 'logo', src: companyResearch },
	bodacc: { kind: 'lucide', Icon: Scale },
}

const DEFAULT_ICON: ProviderIcon = { kind: 'lucide', Icon: Database }

export const CATALOG_KEYS: readonly string[] = Object.keys(ICONS)

export function providerIcon(key: string): ProviderIcon {
	return ICONS[key] ?? DEFAULT_ICON
}
