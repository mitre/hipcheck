<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";

  type AssessmentsData = {
    id: string;
    exposure: string;
    package_source: string;
    patch_candidate: string;
    verdict: "Recommended" | "In Progress" | "Caution" | "Failed" | "Unknown" | "Avoid";
    // updated: Date;
    // action: Button;
    };
  export const data2: AssessmentsData[] = [
    {
    id: "2345678",
    exposure: "lodash@4.17.20",
    package_source: "agency-web",
    patch_candidate: "4.17.21",
    verdict: "Recommended",
    },
    {
    id: "34567890",
    exposure: "minimist@1.2.5",
    package_source: "partner-portal",
    patch_candidate: "1.2.8",
    verdict: "Caution",
    },
    {
    id: "765432",
    exposure: "axios@0.21.1",
    package_source: "claims-service",
    patch_candidate: "Analysis running",
    verdict: "In Progress",
    },
    // ...
    ];

  const assessments = [
    {
    id: "2345678",
    exposure: "lodash@4.17.20",
    details: "CVE-2021-23337",
    package_source: "agency-web",
    source_details: "through api-client",
    patch_candidate: "4.17.21",
    verdict: "Recommended",
    updated: "Today, 9:42 AM",
    action: "Open",
    },
    {
    id: "34567890",
    exposure: "minimist@1.2.5",
    details: "CVE-2021-44906",
    package_source: "partner-portal",
    source_details: "through request-utils",
    patch_candidate: "1.2.8",
    verdict: "Caution",
    updated: "Today, 9:17 AM",
    action: "Open",
    },
    {
    id: "765432",
    exposure: "axios@0.21.1",
    details: "CVE-2021-3749",
    package_source: "claims-service",
    source_details: "direct dependency",
    patch_candidate: "Analysis running",
    verdict: "In Progress",
    updated: "Today, 9:03 AM",
    action: "Status",
    },
    {
    id: "3345678",
    exposure: "axios@0.21.0",
    details: "CVE-2021-3749",
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
<h1> Assessments </h1>
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