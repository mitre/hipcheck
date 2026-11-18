// @ts-nocheck -- executed by Node's built-in TypeScript test runner.
import assert from 'node:assert/strict';
import test from 'node:test';

import { normalizeApiError } from '../src/lib/api/errors.ts';

test('normalizes response errors without exposing arbitrary response content', () => {
	const error = normalizeApiError({
		error: '<script>untrusted server response</script>',
		response: new Response('<script>untrusted server response</script>', { status: 500 })
	});

	assert.deepEqual(error, {
		kind: 'server',
		message: 'Night Vision is temporarily unavailable. Please try again.',
		retryable: true,
		status: 500
	});
});

test('treats malformed JSON from a successful response as an unexpected failure', () => {
	const error = normalizeApiError({
		error: new SyntaxError('private parse details'),
		response: new Response('{', { status: 200 })
	});

	assert.deepEqual(error, {
		kind: 'unexpected',
		message: 'The request could not be completed.',
		retryable: false,
		status: 200
	});
});

test('treats request construction and validation failures as non-retryable', () => {
	assert.deepEqual(normalizeApiError({ error: new TypeError('invalid request') }), {
		kind: 'unexpected',
		message: 'The request could not be completed.',
		retryable: false
	});
});

test('normalizes fetch failures as retryable network failures', () => {
	const error = normalizeApiError({
		error: new TypeError('fetch failed'),
		request: new Request('https://example.test/')
	});

	assert.deepEqual(error, {
		kind: 'network',
		message: 'Unable to reach Night Vision. Check your connection and try again.',
		retryable: true
	});
});

test('does not treat other client failures as network outages', () => {
	const error = normalizeApiError({
		error: new Error('request interceptor failed'),
		request: new Request('https://example.test/')
	});

	assert.deepEqual(error, {
		kind: 'unexpected',
		message: 'The request could not be completed.',
		retryable: false
	});
});

test('does not treat response-processing client errors as backend failures', () => {
	assert.deepEqual(
		normalizeApiError({ error: new TypeError('transformer failed'), response: new Response('{}', { status: 200 }) }),
		{
			kind: 'unexpected',
			message: 'The request could not be completed.',
			retryable: false,
			status: 200
		}
	);
});
