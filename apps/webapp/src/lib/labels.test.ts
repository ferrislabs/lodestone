import { describe, expect, it } from 'vitest'
import {
	contactKindLabel,
	contactMethodLabel,
	pluralize,
	signalKindLabel,
	sourceLabel,
} from './labels'

describe('labels', () => {
	it('translates known signal kinds and keeps unknown ones', () => {
		expect(signalKindLabel('product_detected')).toBe('Produit détecté')
		expect(signalKindLabel('debt_marker')).toBe('Marqueur de dette technique')
		expect(signalKindLabel('distinct_signals')).toBe(
			'Types de signaux distincts',
		)
		expect(signalKindLabel('hiring_page')).toBe('hiring_page')
		expect(signalKindLabel('certificate_seen')).toBe('Certificat observé')
	})

	it('translates sources and keeps proper names', () => {
		expect(sourceLabel('certificate_transparency')).toBe(
			'Transparence des certificats',
		)
		expect(sourceLabel('http_probe')).toBe('Sonde HTTP')
		expect(sourceLabel('boamp')).toBe('BOAMP')
		expect(sourceLabel('france_travail')).toBe('France Travail')
		expect(sourceLabel('hacker_news_hiring')).toBe(
			'Hacker News — Who is hiring?',
		)
		expect(sourceLabel('other')).toBe('other')
	})

	it('translates contact attributes', () => {
		expect(contactMethodLabel('role_guess')).toBe('Adresse de rôle devinée')
		expect(contactKindLabel('nominative')).toBe('Nominatif')
		expect(contactKindLabel('x')).toBe('x')
	})

	it('pluralizes french style', () => {
		expect(pluralize(0, 'facteur', 'facteurs')).toBe('facteur')
		expect(pluralize(1, 'facteur', 'facteurs')).toBe('facteur')
		expect(pluralize(2, 'facteur', 'facteurs')).toBe('facteurs')
	})
})
