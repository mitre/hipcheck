<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";

  const assessments = [
    {
    id: "2345678",
    exposure: "systeminformation@5.3.0",
    details: "CVE-2021-21315",
    package_source: "agency-web",
    source_details: "direct dependency",
    patch_candidate: "5.3.1",
    verdict: "Recommended",
    updated: "Today, 9:42 AM",
    action: "Open",
    },
    {
    id: "4567890",
    exposure: "systeminformation@5.2.6",
    details: "CVE-2021-21315",
    package_source: "agency-web",
    source_details: "through metrics-client",
    patch_candidate: "5.2.7",
    verdict: "Avoid",
    updated: "Today, 9:38 AM",
    action: "Open",
    },
    {
    id: "34567890",
    exposure: "systeminformation@5.2.0",
    details: "CVE-2021-21315",
    package_source: "partner-portal",
    source_details: "through request-utils",
    patch_candidate: "5.3.1",
    verdict: "Caution",
    updated: "Today, 9:17 AM",
    action: "Open",
    },
    {
    id: "765432",
    exposure: "systeminformation@5.1.0",
    details: "CVE-2021-21315",
    package_source: "claims-service",
    source_details: "direct dependency",
    patch_candidate: "Analysis running",
    verdict: "In Progress",
    updated: "Today, 9:03 AM",
    action: "Status",
    },
    {
    id: "3345678",
    exposure: "systeminformation@5.0.11",
    details: "CVE-2021-21315",
    package_source: "claims-service",
    source_details: "direct dependency",
    patch_candidate: "Analysis running",
    verdict: "Unknown",
    updated: "Today, 9:05 AM",
    action: "Status",
    },
    // ...
  ];

</script>

<main>
<h1> Assessments <Badge variant="outline" class="ml-2 align-middle">Example data</Badge></h1>
<p> KEV-linked exposures and their upgrade decisions</p>
<!-- Needs review, processing, completed -->
<!-- Filter the table search bar -->
<Table.Root>
 <Table.Caption>List of recent assessments.</Table.Caption>
 <Table.Header>
  <Table.Row>
   <Table.Head class="w-[100px]">EXPOSURE</Table.Head>
   <Table.Head>PACKAGE SOURCE</Table.Head>
   <Table.Head>PATCH CANDIDATE</Table.Head>
   <Table.Head>VERDICT</Table.Head>
   <Table.Head>UPDATED</Table.Head>
   <Table.Head class="text-end">ACTION</Table.Head>
  </Table.Row>
 </Table.Header>
 <Table.Body>
  {#each assessments as assessment (assessment)}
   <Table.Row>
    <Table.Cell class="font-medium">
      <p style="font-weight: bold;">{assessment.exposure}</p>
      <Badge variant="destructive">KEV affected</Badge>
      {assessment.details}
    </Table.Cell>
    <Table.Cell>
      <p style="font-weight: bold;">{assessment.package_source}</p>
      <p>{assessment.source_details}</p>
    </Table.Cell>
    <Table.Cell>{assessment.patch_candidate}</Table.Cell>
    <Table.Cell>
        {#if assessment.verdict == "Recommended"}
        <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">Recommended</Badge>
        {:else if assessment.verdict == "Caution"}
        <Badge class="bg-amber-100 text-amber-800 dark:bg-blue-950 dark:text-blue-300">Caution</Badge>
        {:else if assessment.verdict == "In Progress"}
        <Badge class="bg-blue-200 text-mist-600 dark:bg-blue-950 dark:text-blue-300">In Progress</Badge>
        {:else if assessment.verdict == "Unknown"}
        <Badge class="bg-mauve-400 text-mist-200 dark:bg-blue-950 dark:text-blue-300">Unknown</Badge>
        {:else if assessment.verdict == "Avoid"}
        <Badge variant="destructive">Avoid</Badge>
        {:else}
        <Badge variant="destructive">Error</Badge>
        {/if}
    </Table.Cell>
    <Table.Cell>{assessment.updated}</Table.Cell>
    <Table.Cell class="text-end">
      <Button href="/assessments/assessmentsId" variant="outline">{assessment.action}</Button>
    </Table.Cell>
   </Table.Row>
  {/each}
 </Table.Body>
 <Table.Footer>
  <Table.Row>
   <Table.Cell colspan={3}>Showing {assessments.length} assessments</Table.Cell>
   <Table.Cell class="text-end">Sorted by highest concern</Table.Cell>
  </Table.Row>
 </Table.Footer>
</Table.Root>

</main>