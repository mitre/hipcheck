import { error } from '@sveltejs/kit';
import { parseSourceIndexState, SOURCE_PAGE_SIZE } from '$lib/source-index';
import { nightVisionApi } from '$lib/server/night-vision-api';
import { pageErrorStatus } from '$lib/server/page-errors';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ url }) => {
	const state = parseSourceIndexState(url.searchParams);
	const result = await nightVisionApi.listPackageSources({
		filter: state.filter,
		query: state.query || undefined,
		sort: state.sort,
		direction: state.direction,
		cursor: state.cursor,
		limit: SOURCE_PAGE_SIZE
	});
	if (!result.ok) error(pageErrorStatus(result.error), result.error.message);

	let hasAnySources = true;
	if (
		result.data.items.length === 0 &&
		!state.cursor &&
		(state.filter !== 'all' || state.query !== '')
	) {
		const unfiltered = await nightVisionApi.listPackageSources({ limit: 1 });
		if (!unfiltered.ok) error(pageErrorStatus(unfiltered.error), unfiltered.error.message);
		hasAnySources = unfiltered.data.items.length > 0;
	} else if (result.data.items.length === 0 && !state.cursor) {
		hasAnySources = false;
	}

	return {
		state,
		sources: result.data.items,
		nextCursor: result.data.nextCursor,
		hasAnySources
	};
};
