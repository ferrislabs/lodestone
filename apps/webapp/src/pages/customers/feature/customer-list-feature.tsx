import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { MOCK_CUSTOMERS } from '#/pages/customers/mocks'
import type { Customer } from '#/pages/customers/types'
import {
	CustomerListUI,
	type Filter,
} from '#/pages/customers/ui/customer-list-ui'

export function CustomerListFeature() {
	const navigate = useNavigate()
	const [customers] = useState<Customer[]>(MOCK_CUSTOMERS)
	const [isLoading] = useState(false)
	// Owned here, not in the presentation layer: this is the first list screen
	// and it sets the pattern for inventory, quotes and invoices.
	const [search, setSearch] = useState('')
	const [filter, setFilter] = useState<Filter>('all')

	// Not implemented: these screens run on mocks, and the handlers logged the
	// customer record to the console, which shipped to production.
	const handleAdd = () => {}

	const handleEdit = (customer: Customer) => {
		void navigate({
			to: '/customers/$customerId',
			params: { customerId: customer.id },
		})
	}

	const handleDelete = (_customer: Customer) => {}

	return (
		<CustomerListUI
			customers={customers}
			search={search}
			filter={filter}
			onSearchChange={setSearch}
			onFilterChange={setFilter}
			isLoading={isLoading}
			onAdd={handleAdd}
			onEdit={handleEdit}
			onDelete={handleDelete}
		/>
	)
}
