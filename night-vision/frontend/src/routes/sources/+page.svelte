<script lang="ts">
	import type { PageData } from './$types';
	import type { PackageSourceListSort } from '$lib/api/generated';
	import { formatTimestamp } from '$lib/format';
	import {
		previousSourceCursor,
		sourceAttentionLabel,
		sourceExposureLabel,
		sourceFilters,
		sourceIndexHref,
		sourceSortHref,
		sourceSorts,
		SOURCE_PAGE_SIZE
	} from '$lib/source-index';
	import StatusBadge from './status-badge.svelte';

	let { data }: { data: PageData } = $props();

	const sortAria = (sort: PackageSourceListSort): 'ascending' | 'descending' | 'none' =>
		data.state.sort === sort ? (data.state.direction === 'asc' ? 'ascending' : 'descending') : 'none';
	const sortAction = (sort: PackageSourceListSort): string =>
		`Sort by ${sourceSorts.find((option) => option.value === sort)?.label.toLowerCase()}, ${
			data.state.sort === sort && data.state.direction === 'desc' ? 'ascending' : 'descending'
		}`;
	const sortMark = (sort: PackageSourceListSort): string =>
		data.state.sort === sort ? (data.state.direction === 'asc' ? '↑' : '↓') : '';
</script>

<svelte:head>
	<title>Package sources | Night Vision</title>
</svelte:head>

<section class="mx-auto max-w-7xl space-y-6">
	<div class="flex flex-wrap items-start justify-between gap-4">
		<div>
			<h1 class="text-3xl font-semibold">Package sources</h1>
			<p class="mt-2 text-sm text-muted-foreground">
				Submitted NPM sources and their resolution and exposure state.
			</p>
		</div>
		<a
			href="/sources/new"
			class="inline-flex min-h-11 items-center rounded-md bg-cyan-900 px-4 py-2 font-medium text-white hover:bg-cyan-800 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-700"
		>
			Add package source
		</a>
	</div>

	<nav aria-label="Source lifecycle views" class="flex flex-wrap gap-2">
		{#each sourceFilters as filter}
			<a
				href={sourceIndexHref(data.state, { filter: filter.value })}
				aria-current={data.state.filter === filter.value ? 'page' : undefined}
				class="rounded-md border px-3 py-2 text-sm font-medium hover:bg-slate-100 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-700 dark:hover:bg-slate-800"
				class:bg-cyan-900={data.state.filter === filter.value}
				class:text-white={data.state.filter === filter.value}
			>
				{filter.label}
			</a>
		{/each}
	</nav>

	<form method="GET" action="/sources" role="search" class="flex flex-wrap items-end gap-3 rounded-lg border p-4">
		<input type="hidden" name="filter" value={data.state.filter} />
		<div class="min-w-48 flex-1">
			<label for="source-query" class="mb-1 block text-sm font-medium">Source name</label>
			<input
				id="source-query"
				name="query"
				type="search"
				maxlength="100"
				value={data.state.query}
				placeholder="Filter by source name"
				class="min-h-11 w-full rounded-md border bg-background px-3"
			/>
		</div>
		<div>
			<label for="source-sort" class="mb-1 block text-sm font-medium">Sort by</label>
			<select id="source-sort" name="sort" class="min-h-11 w-full rounded-md border bg-background px-3">
				{#each sourceSorts as option}
					<option value={option.value} selected={data.state.sort === option.value}>{option.label}</option>
				{/each}
			</select>
		</div>
		<div>
			<label for="source-direction" class="mb-1 block text-sm font-medium">Direction</label>
			<select id="source-direction" name="direction" class="min-h-11 w-full rounded-md border bg-background px-3">
				<option value="desc" selected={data.state.direction === 'desc'}>Descending</option>
				<option value="asc" selected={data.state.direction === 'asc'}>Ascending</option>
			</select>
		</div>
		<button type="submit" class="min-h-11 rounded-md border px-4 font-medium hover:bg-slate-100 dark:hover:bg-slate-800">
			Apply
		</button>
		{#if data.state.query}
			<a href={sourceIndexHref(data.state, { query: '' })} class="min-h-11 px-2 py-3 text-sm underline">Clear search</a>
		{/if}
	</form>

	{#if data.sources.length === 0}
		<div class="rounded-lg border p-8 text-center">
			{#if data.state.cursor}
				<h2 class="text-lg font-semibold">No sources on this page</h2>
				<p class="mt-2 text-sm text-muted-foreground">The list may have changed since you opened it.</p>
				<a href={sourceIndexHref(data.state, { cursor: undefined })} class="mt-4 inline-block underline">Return to the first page</a>
			{:else if !data.hasAnySources}
				<h2 class="text-lg font-semibold">No package sources submitted yet</h2>
				<p class="mt-2 text-sm text-muted-foreground">Add an NPM package source to begin reviewing exposures.</p>
				<a href="/sources/new" class="mt-4 inline-block underline">Add package source</a>
			{:else}
				<h2 class="text-lg font-semibold">No sources match this view</h2>
				<p class="mt-2 text-sm text-muted-foreground">Try a different lifecycle view or source name.</p>
				<a href="/sources" class="mt-4 inline-block underline">Show all sources</a>
			{/if}
		</div>
	{:else}
		<p class="text-sm text-muted-foreground">
			Showing {data.sources.length} source{data.sources.length === 1 ? '' : 's'} on page {Math.floor(Number(data.state.cursor ?? 0) / SOURCE_PAGE_SIZE) + 1}.
			Exposure counts reflect current CVE and KEV data; unavailable counts are not zero.
		</p>

		<div class="hidden overflow-x-auto rounded-lg border md:block">
			<table class="w-full min-w-[1100px] text-left text-sm">
				<caption class="sr-only">Package source summaries</caption>
				<thead class="bg-slate-100 dark:bg-slate-800">
					<tr>
						<th scope="col" aria-sort={sortAria('identity')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'identity')} aria-label={sortAction('identity')} class="underline-offset-2 hover:underline focus-visible:underline">Source {sortMark('identity')}</a></th>
						<th scope="col" aria-sort={sortAria('lifecycle')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'lifecycle')} aria-label={sortAction('lifecycle')} class="underline-offset-2 hover:underline focus-visible:underline">Lifecycle {sortMark('lifecycle')}</a></th>
						<th scope="col" aria-sort={sortAria('activity')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'activity')} aria-label={sortAction('activity')} class="underline-offset-2 hover:underline focus-visible:underline">Latest activity {sortMark('activity')}</a></th>
						<th scope="col" aria-sort={sortAria('resolution-time')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'resolution-time')} aria-label={sortAction('resolution-time')} class="underline-offset-2 hover:underline focus-visible:underline">Resolution time {sortMark('resolution-time')}</a></th>
						<th scope="col" aria-sort={sortAria('reachable-packages')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'reachable-packages')} aria-label={sortAction('reachable-packages')} class="underline-offset-2 hover:underline focus-visible:underline">Reachable packages {sortMark('reachable-packages')}</a></th>
						<th scope="col" aria-sort={sortAria('exposures')} class="px-4 py-3"><a href={sourceSortHref(data.state, 'exposures')} aria-label={sortAction('exposures')} class="underline-offset-2 hover:underline focus-visible:underline">KEV exposures {sortMark('exposures')}</a></th>
						<th scope="col" class="px-4 py-3">Attention</th>
						<th scope="col" class="px-4 py-3">Action</th>
					</tr>
				</thead>
				<tbody>
					{#each data.sources as source (source.id)}
						<tr class="border-t align-top">
							<td class="px-4 py-4"><a href={`/sources/${source.id}`} class="font-semibold underline underline-offset-2">{source.displayName}</a><span class="mt-1 block text-xs uppercase text-muted-foreground">{source.ecosystem}</span><code class="mt-1 block break-all text-xs text-muted-foreground">{source.id}</code></td>
							<td class="px-4 py-4"><StatusBadge status={source.lifecycle} /></td>
							<td class="px-4 py-4"><time datetime={source.activityAt}>{formatTimestamp(source.activityAt)}</time></td>
							<td class="px-4 py-4">{#if source.resolutionAt}<time datetime={source.resolutionAt}>{formatTimestamp(source.resolutionAt)}</time>{:else}Not complete{/if}</td>
							<td class="px-4 py-4">{source.reachablePackageCount ?? 'Not available'}</td>
							<td class="px-4 py-4">{sourceExposureLabel(source)}</td>
							<td class="px-4 py-4">{sourceAttentionLabel(source)}</td>
							<td class="px-4 py-4"><a href={`/sources/${source.id}`} aria-label={`View ${source.displayName}`} class="font-medium underline underline-offset-2">View source</a></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<div class="space-y-3 md:hidden">
			{#each data.sources as source (source.id)}
				<article class="rounded-lg border p-4">
					<div class="flex flex-wrap items-start justify-between gap-3">
						<div class="min-w-0"><h2 class="break-words font-semibold">{source.displayName}</h2><p class="text-xs uppercase text-muted-foreground">{source.ecosystem}</p><code class="mt-1 block break-all text-xs text-muted-foreground">{source.id}</code></div>
						<StatusBadge status={source.lifecycle} />
					</div>
					<dl class="mt-4 grid grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] gap-x-3 gap-y-2 text-sm">
						<dt>Latest activity</dt><dd><time datetime={source.activityAt}>{formatTimestamp(source.activityAt)}</time></dd>
						<dt>Resolution time</dt><dd>{#if source.resolutionAt}<time datetime={source.resolutionAt}>{formatTimestamp(source.resolutionAt)}</time>{:else}Not complete{/if}</dd>
						<dt>Reachable packages</dt><dd>{source.reachablePackageCount ?? 'Not available'}</dd>
						<dt>KEV exposures</dt><dd>{sourceExposureLabel(source)}</dd>
						<dt>Attention</dt><dd>{sourceAttentionLabel(source)}</dd>
					</dl>
					<a href={`/sources/${source.id}`} class="mt-4 inline-flex min-h-11 items-center font-medium underline underline-offset-2">View source</a>
				</article>
			{/each}
		</div>
	{/if}

	{#if data.state.cursor || data.nextCursor}
		<nav aria-label="Source pages" class="flex items-center justify-between gap-4 text-sm">
			{#if data.state.cursor}
				<a href={sourceIndexHref(data.state, { cursor: previousSourceCursor(data.state.cursor) })} class="rounded-md border px-4 py-3 font-medium">Previous page</a>
			{:else}
				<span></span>
			{/if}
			{#if data.nextCursor}
				<a href={sourceIndexHref(data.state, { cursor: data.nextCursor })} class="rounded-md border px-4 py-3 font-medium">Next page</a>
			{/if}
		</nav>
	{/if}
</section>
