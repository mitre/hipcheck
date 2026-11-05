<script lang="ts">
	import { displayEvidenceText } from '$lib/evidence';

	let { value, preserveNewlines = true }: { value: string; preserveNewlines?: boolean } = $props();
</script>

<span class:multiline={preserveNewlines}>{displayEvidenceText(value, preserveNewlines)}</span>

<style>
	.multiline {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
