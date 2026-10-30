import type {
	PackageSourceListDirection,
	PackageSourceListFilter,
	PackageSourceListSort,
	PackageSourceSummary
} from './api/generated';

export const SOURCE_PAGE_SIZE = 25;

export type SourceIndexState = {
	filter: PackageSourceListFilter;
	query: string;
	sort: PackageSourceListSort;
	direction: PackageSourceListDirection;
	cursor?: string;
};

export const sourceFilters: { value: PackageSourceListFilter; label: string }[] = [
	{ value: 'all', label: 'All' },
	{ value: 'needs-attention', label: 'Needs attention' },
	{ value: 'processing', label: 'Processing' },
	{ value: 'failed', label: 'Failed' }
];

export const sourceSorts: { value: PackageSourceListSort; label: string }[] = [
	{ value: 'activity', label: 'Latest activity' },
	{ value: 'identity', label: 'Source name' },
	{ value: 'lifecycle', label: 'Lifecycle' },
	{ value: 'resolution-time', label: 'Resolution time' },
	{ value: 'reachable-packages', label: 'Reachable packages' },
	{ value: 'exposures', label: 'KEV exposures' }
];

export const parseSourceIndexState = (params: URLSearchParams): SourceIndexState => {
	const filter = params.get('filter');
	const sort = params.get('sort');
	const direction = params.get('direction');
	const cursor = params.get('cursor');
	return {
		filter: sourceFilters.find((option) => option.value === filter)?.value ?? 'all',
		query: Array.from(params.get('query')?.trim() ?? '').slice(0, 100).join(''),
		sort: sourceSorts.find((option) => option.value === sort)?.value ?? 'activity',
		direction: direction === 'asc' ? 'asc' : 'desc',
		cursor:
			cursor && /^\d+$/.test(cursor) && Number(cursor) > 0 && Number(cursor) <= 10_000
				? String(Number(cursor))
				: undefined
	};
};

export const sourceIndexHref = (
	state: SourceIndexState,
	changes: Partial<SourceIndexState> = {}
): string => {
	const next = { ...state, ...changes };
	if (Object.keys(changes).some((key) => key !== 'cursor') && !('cursor' in changes)) {
		next.cursor = undefined;
	}
	const params = new URLSearchParams();
	if (next.filter !== 'all') params.set('filter', next.filter);
	if (next.query) params.set('query', next.query);
	if (next.sort !== 'activity') params.set('sort', next.sort);
	if (next.direction !== 'desc') params.set('direction', next.direction);
	if (next.cursor) params.set('cursor', next.cursor);
	return `/sources${params.size ? `?${params}` : ''}`;
};

export const sourceSortHref = (state: SourceIndexState, sort: PackageSourceListSort): string =>
	sourceIndexHref(state, {
		sort,
		direction: state.sort === sort && state.direction === 'desc' ? 'asc' : 'desc'
	});

export const previousSourceCursor = (cursor: string | undefined): string | undefined => {
	const offset = Number(cursor ?? 0);
	return offset > SOURCE_PAGE_SIZE ? String(offset - SOURCE_PAGE_SIZE) : undefined;
};

export const sourceExposureLabel = (source: PackageSourceSummary): string => {
	switch (source.exposureStatus) {
		case 'available':
			return source.exposureCount == null
				? 'Unavailable'
				: source.exposureCount === 0
					? '0 — no current KEV exposures'
					: String(source.exposureCount);
		case 'processing':
			return 'Pending';
		case 'failed':
			return 'Unavailable after failure';
		case 'cancelled':
			return 'Cancelled';
		case 'unavailable':
			return 'Data unavailable';
	}
};

export const sourceAttentionLabel = (source: PackageSourceSummary): string => {
	switch (source.attention) {
		case 'none':
			return 'None';
		case 'warnings':
			return 'Warnings';
		case 'failed':
			return 'Failed';
		case 'exposure-data-unavailable':
			return 'Exposure data unavailable';
	}
};
