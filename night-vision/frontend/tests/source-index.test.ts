// @ts-nocheck -- executed by Node's built-in TypeScript test runner.
import assert from 'node:assert/strict';
import test from 'node:test';

import {
	parseSourceIndexState,
	previousSourceCursor,
	sourceAttentionLabel,
	sourceExposureLabel,
	sourceIndexHref,
	sourceSortHref
} from '../src/lib/source-index.ts';

const source = {
	id: 'source-1',
	displayName: 'Example application',
	ecosystem: 'npm',
	lifecycle: 'completed',
	createdAt: '2026-01-01T00:00:00Z',
	activityAt: '2026-01-01T00:01:00Z',
	resolutionAt: '2026-01-01T00:01:00Z',
	reachablePackageCount: 4,
	exposureCount: 0,
	exposureStatus: 'available',
	attention: 'none'
};

test('normalizes unsupported controls and bounds source search before calling the API', () => {
	const state = parseSourceIndexState(
		new URLSearchParams({ filter: 'unknown', sort: 'created-at', direction: 'sideways', cursor: 'oops', query: `  ${'x'.repeat(120)}  ` })
	);
	assert.deepEqual(state, {
		filter: 'all',
		query: 'x'.repeat(100),
		sort: 'activity',
		direction: 'desc',
		cursor: undefined
	});
	assert.equal(parseSourceIndexState(new URLSearchParams({ cursor: '0' })).cursor, undefined);
});

test('filter and sort links preserve search but restart pagination', () => {
	const state = parseSourceIndexState(
		new URLSearchParams({ filter: 'failed', query: '@scope/app', sort: 'identity', direction: 'asc', cursor: '25' })
	);
	assert.equal(
		sourceIndexHref(state, { filter: 'processing' }),
		'/sources?filter=processing&query=%40scope%2Fapp&sort=identity&direction=asc'
	);
	assert.equal(
		sourceSortHref(state, 'identity'),
		'/sources?filter=failed&query=%40scope%2Fapp&sort=identity'
	);
	assert.equal(
		sourceSortHref(state, 'resolution-time'),
		'/sources?filter=failed&query=%40scope%2Fapp&sort=resolution-time'
	);
	assert.equal(previousSourceCursor('25'), undefined);
	assert.equal(previousSourceCursor('50'), '25');
});

test('only an available zero is described as zero exposures', () => {
	assert.equal(sourceExposureLabel(source), '0 — no current KEV exposures');
	assert.equal(sourceExposureLabel({ ...source, exposureCount: null, exposureStatus: 'unavailable' }), 'Data unavailable');
	assert.equal(sourceExposureLabel({ ...source, exposureCount: null, exposureStatus: 'processing' }), 'Pending');
	assert.equal(sourceExposureLabel({ ...source, exposureCount: null, exposureStatus: 'failed' }), 'Unavailable after failure');
	assert.equal(sourceExposureLabel({ ...source, exposureCount: null, exposureStatus: 'cancelled' }), 'Cancelled');
	assert.equal(sourceExposureLabel({ ...source, exposureCount: null }), 'Unavailable');
});

test('attention is expressed in text', () => {
	assert.equal(sourceAttentionLabel({ ...source, attention: 'warnings' }), 'Warnings');
	assert.equal(sourceAttentionLabel({ ...source, attention: 'failed' }), 'Failed');
	assert.equal(sourceAttentionLabel({ ...source, attention: 'exposure-data-unavailable' }), 'Exposure data unavailable');
});
