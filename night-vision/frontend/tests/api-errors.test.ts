// @ts-nocheck -- executed by Node's built-in TypeScript test runner.
import assert from 'node:assert/strict';
import test from 'node:test';

import { normalizeApiError } from '../src/lib/api/errors.ts';

test('normalizes response errors without exposing arbitrary response content', () => {
	const error = normalizeApiError(new Response('<script>untrusted server response</script>', { status: 500 }));

	assert.deepEqual(error, {
		kind: 'server',
		message: 'Night Vision is temporarily unavailable. Please try again.',
		retryable: true,
		status: 500
	});
});

test('normalizes missing responses as retryable network failures', () => {
	assert.deepEqual(normalizeApiError(), {
		kind: 'network',
		message: 'Unable to reach Night Vision. Check your connection and try again.',
		retryable: true
	});
});
