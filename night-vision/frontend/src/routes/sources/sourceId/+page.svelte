<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";

    const assessments = [
    {
    id: "2345678",
    affected_package: "lodash@4.17.20",
    details: "CVE-2021-23337",
    reachability: "through api-client",
    package_source: "agency-web",
    patch_candidate: "4.17.21",
    assessment: "Recommended",
    action: "Open",
    },
    {
    id: "34567890",
    affected_package: "qs@6.5.1",
    details: "CVE-2022-24999",
    reachability: "through body-parser",
    package_source: "partner-portal",
    patch_candidate: "6.5.3",
    assessment: "Avoid",
    action: "Open",
    },
    // ...
  ];
 </script>

<main>
  <h2 style="color:grey;"><a href="/sources">Example sources</a> / agency-web</h2>
  <h1 style="float:left">agency-web <Badge variant="outline" class="ml-2 align-middle">Example data</Badge></h1>

  <Button href="/sources/new" style="float:right" class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
    View submitted manifest
  </Button>
<br>
<br>

<!-- <p>Submitted NPM manifests and their resolution status.</p> -->
<br>
<p>NPM package.json - submitted Sep 4, 2026</p>
<br>
<Card.Root>
  <Card.Header>
    <Card.Title> </Card.Title>
  </Card.Header>
    <Card.Content>
      <div class="flex h-5 items-center gap-4 text-sm">
        <div>
            <p style="font-weight: bold;"><Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">Completed</Badge>Dependency resolution finished</p>
            <p>Night Vision found KEV-linked exposures that need an upgrade assessment.</p>
            <p style="font-weight: bold; color: green;" ><a href="/sources">View resolution details</a></p>
        </div>
      <Separator orientation="vertical" />
        <div>
            <p style="font-size: 10px;">LAST RESOLVED</p>
            <p style="font-weight: bold;">9:11 AM Today</p>
        </div>
      <Separator orientation="vertical" />
        <div>
            <p style="font-size: 10px;">REACHABLE PACKAGES KEV EXPOSURES</p>
            <p style="font-weight: bold;">142 Direct and Transitive</p>
            <p style="font-weight: bold;">2 Need Review</p>
        </div>
      </div>  
  </Card.Content>
  <Card.Footer>
    <br>
  </Card.Footer>
</Card.Root>

<br>
<h3>KEV-linked exposures</h3>
<p>Open an assessment to compare patch candidates and review the evidence.</p>

<Card.Root>
    <Card.Content>
      <Table.Root>
        <Table.Caption>List of recent assessments.</Table.Caption>
        <Table.Header>
        <Table.Row style="font-size: 13px; color:slategrey;">
        <Table.Head class="w-[100px]">AFFECTED PACKAGE</Table.Head>
        <Table.Head>REACHABILITY</Table.Head>
        <Table.Head>PATCH CANDIDATE</Table.Head>
        <Table.Head>ASSESSMENT</Table.Head>
        <Table.Head class="text-end">ACTION</Table.Head>
        </Table.Row>
        </Table.Header>
        <Table.Body>
        {#each assessments as assessment (assessment)}
        <Table.Row>
            <Table.Cell class="font-medium">
            <p style="font-weight: bold;">{assessment.affected_package}</p>
            <Badge variant="destructive">KEV affected</Badge>
            {assessment.details}
            </Table.Cell>
            <Table.Cell>
            <p style="font-weight: bold;">{assessment.reachability}</p>
            </Table.Cell>
            <Table.Cell>{assessment.patch_candidate}</Table.Cell>
            <Table.Cell>
                {#if assessment.assessment == "Recommended"}
                <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">Recommended</Badge>
                {:else if assessment.assessment == "Caution"}
                <Badge class="bg-amber-100 text-amber-800 dark:bg-blue-950 dark:text-blue-300">Caution</Badge>
                {:else if assessment.assessment == "In Progress"}
                <Badge class="bg-blue-200 text-mist-600 dark:bg-blue-950 dark:text-blue-300">In Progress</Badge>
                {:else if assessment.assessment == "Unknown"}
                <Badge class="bg-mauve-400 text-mist-200 dark:bg-blue-950 dark:text-blue-300">Unknown</Badge>
                {:else if assessment.assessment == "Avoid"}
                <Badge class="bg-amber-100 text-amber-800 dark:bg-blue-950 dark:text-blue-300">Avoid</Badge>
                {:else}
                <Badge variant="destructive">Error</Badge>
                {/if}
            </Table.Cell>
            <Table.Cell class="text-end">
            <Button href="/assessments/assessmentsId" variant="outline">{assessment.action}</Button>
            </Table.Cell>
        </Table.Row>
        {/each}
        </Table.Body>
        <Table.Footer>
        <Table.Row>
        </Table.Row>
        </Table.Footer>
      </Table.Root>
  </Card.Content>
  <Card.Footer>
    <br>
  </Card.Footer>
</Card.Root>

</main>