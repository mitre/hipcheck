<script lang="ts">
	import * as Card from '$lib/components/ui/card/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import StatusBadge from '../status-badge.svelte';
	import { formatFailureKind, formatTimestamp } from '../format';
	import { invalidateAll } from '$app/navigation';

	let { data } = $props();
	const source = $derived(data.source);
	const resolving = $derived(source.status === 'pending' || source.status === 'processing');

	// Reload every two seconds until resolution finishes, so the page follows it live.
	$effect(() => {
		if (!resolving) return;
		const timer = setInterval(() => invalidateAll(), 2000);
		return () => clearInterval(timer);
	});
</script>

<h2 style="color: grey;"><a href="/packagesources">Package sources</a> / {source.title}</h2>
<div class="flex items-center gap-3">
	<h1>{source.title}</h1>
	<StatusBadge status={source.status} />
</div>
{#if source.title !== source.fileName}
	<p class="text-sm text-muted-foreground">{source.fileName}</p>
{/if}

<Card.Root class="mt-4">
	<Card.Content>
		<div class="flex flex-wrap items-center gap-6 text-sm">
			<div>
				<p class="text-[10px] text-muted-foreground">SUBMITTED</p>
				<p class="font-bold">{formatTimestamp(source.createdAt)}</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">FINISHED</p>
				<p class="font-bold">{formatTimestamp(source.finishedAt)}</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">REACHABLE PACKAGES</p>
				<p class="font-bold">
					{source.reachablePackages ?? '—'}
					{#if source.reachablePackages !== null}<span class="font-normal">direct and transitive</span>{/if}
				</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">KEV EXPOSURES</p>
				<p class="font-bold">{data.exposures?.length ?? '—'}</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">ATTEMPT</p>
				<p class="font-bold">{source.attempt}</p>
			</div>
		</div>
		{#if source.failure}
			<p class="mt-4 text-sm text-destructive">
				{formatFailureKind(source.failure.kind)} failure: {source.failure.diagnostic}
			</p>
		{/if}
	</Card.Content>
</Card.Root>

{#if source.warnings.length > 0}
	<h3 class="mt-6">Warnings</h3>
	<ul class="list-disc pl-6 text-sm">
		{#each source.warnings as warning, index (index)}
			<li>
				<span class="font-medium">{warning.dependencyName}</span> ({warning.specificationKind}): {warning.message}
			</li>
		{/each}
	</ul>
{/if}

<h3 class="mt-6">KEV-linked exposures</h3>
<p class="text-sm">
	Reachable package versions affected by a CVE in CISA's Known Exploited Vulnerabilities catalog.
</p>

{#if data.exposuresError}
	<p class="mt-2 text-sm">Exposure data is unavailable right now: {data.exposuresError}</p>
{:else if source.failure}
	<p class="mt-2 text-sm">No exposures to show: dependency resolution failed.</p>
{:else if data.exposures === null}
	<p class="mt-2 text-sm">Exposures appear once dependency resolution completes.</p>
{:else if data.exposures.length === 0}
	<p class="mt-2 text-sm">No KEV-linked exposures were found in this source's reachable packages.</p>
{:else}
	<Card.Root class="mt-2">
		<Card.Content>
			<Table.Root>
				<Table.Header>
					<Table.Row>
						<Table.Head>AFFECTED PACKAGE</Table.Head>
						<Table.Head>CVE</Table.Head>
						<Table.Head>REACHABILITY</Table.Head>
						<Table.Head>ADDED TO KEV</Table.Head>
						<Table.Head class="text-end">ASSESSMENT</Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each data.exposures as exposure (exposure.package + exposure.cveId)}
						<Table.Row>
							<Table.Cell>
								<p class="font-bold">{exposure.package}</p>
								<Badge variant="destructive">KEV affected</Badge>
							</Table.Cell>
							<Table.Cell>
								<p class="font-medium">{exposure.cveId}</p>
								{#if exposure.vulnerabilityName}
									<p class="text-xs whitespace-normal text-muted-foreground">{exposure.vulnerabilityName}</p>
								{/if}
							</Table.Cell>
							<Table.Cell>{exposure.reachability}</Table.Cell>
							<Table.Cell>{exposure.kevDateAdded}</Table.Cell>
							<Table.Cell class="text-end">
								<Button href="/assessments/assessmentsId" variant="outline" size="sm">Example assessment</Button>
							</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</Card.Content>
	</Card.Root>
	<p class="mt-2 text-xs text-muted-foreground">
		Upgrade assessments aren't on the website yet, so "Example assessment" opens the example assessment page.
	</p>
{/if}
