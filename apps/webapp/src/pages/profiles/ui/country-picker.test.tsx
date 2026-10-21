import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import {
	COUNTRY_CODES,
	countryName,
	searchCountries,
	withCountry,
	withoutCountry,
} from '#/pages/profiles/lib/countries'
import { CountryPicker } from '#/pages/profiles/ui/country-picker'

afterEach(cleanup)

let latest: string[] = []

function Harness({ initial }: { initial: string[] }) {
	const [codes, setCodes] = useState(initial)
	const [query, setQuery] = useState('')
	latest = codes
	return (
		<CountryPicker
			id="countries"
			codes={codes}
			query={query}
			onQueryChange={setQuery}
			onAdd={(code) => {
				setCodes((current) => withCountry(current, code))
				setQuery('')
			}}
			onRemove={(code) => setCodes((current) => withoutCountry(current, code))}
		/>
	)
}

const FIRST = COUNTRY_CODES[0] as string
const SECOND = COUNTRY_CODES[1] as string

describe('country picker', () => {
	it('shows the chosen countries by name and removes one', () => {
		render(<Harness initial={[FIRST, SECOND]} />)
		expect(screen.getByText(countryName(FIRST))).toBeTruthy()
		fireEvent.click(
			screen.getByRole('button', { name: `Retirer ${countryName(FIRST)}` }),
		)
		expect(latest).toEqual([SECOND])
	})

	it('searches countries by name and adds the clicked one', () => {
		render(<Harness initial={[]} />)
		const name = countryName(SECOND)
		const query = name.slice(0, 4)
		const input = screen.getByRole('combobox', { name: 'Pays' })
		expect(input.getAttribute('aria-expanded')).toBe('false')
		fireEvent.change(input, { target: { value: query } })
		expect(input.getAttribute('aria-expanded')).toBe('true')
		const options = screen.getAllByRole('option')
		expect(options.map((option) => option.textContent)).toEqual(
			searchCountries(query, []).map((country) => country.name),
		)
		fireEvent.click(screen.getByRole('option', { name }))
		expect(latest).toEqual([SECOND])
		expect(screen.queryAllByRole('option')).toHaveLength(0)
	})

	it('adds the first match on Enter and ignores chosen countries', () => {
		render(<Harness initial={[FIRST]} />)
		const input = screen.getByRole('combobox', { name: 'Pays' })
		const query = countryName(SECOND).slice(0, 4)
		fireEvent.change(input, { target: { value: query } })
		const expected = searchCountries(query, [FIRST])[0]
		fireEvent.keyDown(input, { key: 'Enter' })
		expect(latest).toEqual([FIRST, expected?.code])
	})

	it('says when nothing matches', () => {
		render(<Harness initial={[]} />)
		fireEvent.change(screen.getByRole('combobox', { name: 'Pays' }), {
			target: { value: 'zzzzzz' },
		})
		expect(screen.getByText('Aucun pays trouvé.')).toBeTruthy()
	})
})
