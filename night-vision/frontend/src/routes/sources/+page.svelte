<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";

  const packages = [
    {
    id: "2345678",
    package_source: "agency-web",
    details: "package.json - submitted Sep 4",
    status: "Completed",
    last_resolved: "Today, 9:11 AM",
    reachable: 142,
    kev_exposures: 2, 
    action: "View source",
    },
    {
    id: "12345678",
    package_source: "claims-service",
    details: "package.json - submitted Sep 4",
    status: "Resolving",
    last_resolved: "In progress",
    reachable: "—",
    kev_exposures: "—", 
    action: "View status",
    },
    {
    id: "90987654",
    package_source: "partner-portal",
    details: "package.json - submitted Sep 3",
    status: "Failed",
    last_resolved: "Yesterday",
    reachable: "—",
    kev_exposures: "—", 
    action: "View error",
    },
    // ...
  ];
</script>

<main>
<h1>Package Sources</h1>
<p style="display:flex; float:left">Submitted NPM manifests and their resolution status.</p>

<Button href="/sources/new" style="display:flex; float:right" class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
    + Add Package Source
</Button>

<!-- Needs all, needs attention, processing, failed -->
<!-- Filter the table search bar -->
<Table.Root>
 <Table.Caption>List of recent package sources.</Table.Caption>
 <Table.Header>
  <Table.Row>
   <Table.Head>PACKAGE SOURCE</Table.Head>
   <Table.Head>STATUS</Table.Head>
   <Table.Head>LAST RESOLVED</Table.Head>
   <Table.Head>REACHABLE</Table.Head>
   <Table.Head>KEV EXPOSURES</Table.Head>
   <Table.Head class="text-end">ACTION</Table.Head>
  </Table.Row>
 </Table.Header>
 <Table.Body>
  {#each packages as pack (pack)}
   <Table.Row>
    <Table.Cell class="font-medium">
      <p style="font-weight: bold;">{pack.package_source}</p>
      {pack.details}
    </Table.Cell>
    <Table.Cell>
        {#if pack.status == "Completed"}
        <Badge class="bg-green-50 text-green-700 dark:bg-green-950 dark:text-green-300">Completed</Badge>
        {:else if pack.status == "Resolving"}
        <Badge class="bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300">Resolving</Badge>
        {:else}
        <Badge variant="destructive">Failed</Badge>
        {/if}
    </Table.Cell>
    <Table.Cell>{pack.last_resolved}</Table.Cell>
    <Table.Cell>{pack.reachable}</Table.Cell>
    <Table.Cell>{pack.kev_exposures}</Table.Cell>
    <Table.Cell class="text-end">
      <Button href="/sources/sourceId" variant="outline">{pack.action}</Button>
    </Table.Cell>
   </Table.Row>
  {/each}
 </Table.Body>
 <Table.Footer>
  <Table.Row>
   <Table.Cell colspan={3}>Showing 2 assessments</Table.Cell>
   <Table.Cell class="text-end">Sorted by highest concern</Table.Cell>
  </Table.Row>
 </Table.Footer>
</Table.Root>

</main>