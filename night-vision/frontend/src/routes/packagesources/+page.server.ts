import { error } from '@sveltejs/kit';
import type { ApiError } from '$lib/api/errors';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { PageServerLoad } from './$types';

// SvelteKit's error() only accepts 400-599. Network failures carry no status
// and an unreadable success response carries a 2xx one, so both become 502.
const pageErrorStatus = ({ status }: ApiError): number =>
	status !== undefined && status >= 400 && status <= 599 ? status : 502;

export const load: PageServerLoad = async () => {
	const result = await nightVisionApi.listPackageSources();
	if (!result.ok) error(pageErrorStatus(result.error), result.error.message);

	return {
		packageSources: result.data.items
	};
};
