<script lang="ts">
    import Button from "$lib/components/ui/button/button.svelte";
    import Plus from "@lucide/svelte/icons/plus";
    import DataTable from "./data-table.svelte";
    import { columns } from "./columns.js";

    let { data } = $props();

    
</script>
<h1>Package Sources</h1>
<div class="flex items-center justify-between gap-4">
    <p>Submitted NPM manifests and their resolution status.</p>
    <Button href="/sources/new" class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
        <Plus data-icon="inline-start" />
        Add Package Source
    </Button>
</div>


<DataTable data={data.packageSources} {columns} />