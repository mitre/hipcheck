import { createClient } from '$lib/api/generated/client';
import type { Health } from '$lib/api/generated';
import type { PageServerLoad } from './$types';
import { env } from '$env/dynamic/private';

const client = createClient({
	baseUrl: env.API_BASE_URL ?? 'http://127.0.0.1:8080',
});

export const load: PageServerLoad = async () => {
	const { data, error } = await client.get<Health>({ url: '/health' });

	if (error) {
		throw new Error(`Failed to retrieve API health: ${String(error)}`);
	}

	return { health: data };
};
