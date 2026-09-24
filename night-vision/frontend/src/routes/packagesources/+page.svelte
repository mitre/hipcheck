<script lang="ts">
    import Button from "$lib/components/ui/button/button.svelte";
    import DataTable from "./data-table.svelte";
    import { columns } from "./columns.js";

    let { data } = $props();

    
</script>
<h1>Package Sources</h1>
<p>Submitted NPM manifests and their resolution status.</p>

<Button>
    Add Package Source
</Button>


<DataTable data={data.packageSources} {columns} />