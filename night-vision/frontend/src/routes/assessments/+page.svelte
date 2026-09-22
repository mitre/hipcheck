<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Alert from "$lib/components/ui/alert/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";
  import * as Accordion from "$lib/components/ui/accordion/index.js";
  import DataTable from "./data-table.svelte";
  import { columns } from "./columns.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";

 
  //let { data } = $props();
//   import { Badge, Table, Card } from "$lib/components/ui";

  let message = 'Hello is this working';
  

  // type Payment = {
  //   id: string;
  //   amount: number;
  //   status: "pending" | "processing" | "success" | "failed";
  //   email: string;
  //   };
    
  //   export const data: Payment[] = [
  //   {
  //   id: "728ed52f",
  //   amount: 100,
  //   status: "pending",
  //   email: "m@example.com",
  //   },
  //   {
  //   id: "489e1d42",
  //   amount: 125,
  //   status: "processing",
  //   email: "example@gmail.com",
  //   },    
  //   {
  //   id: "43763565",
  //   amount: 124367545,
  //   status: "success",
  //   email: "retyui@gmail.com",
  //   },
  //   // ...
  //   ];
  // let { id }: { id: string } = $props();

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
    exposure: "loadsh@4.17.20",
    package_source: "agency-web",
    patch_candidate: "4.17.20",
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
  // let { id2 }: { id2: string } = $props();

  const assessments = [
    {
    id: "2345678",
    exposure: "loadsh@4.17.20",
    details: "CVE-2021-23337",
    package_source: "agency-web",
    source_details: "through api-client",
    patch_candidate: "4.17.20",
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
    exposure: "axios@2.21.1",
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
 
  const invoices = [
    {
    invoice: "INV001",
    paymentStatus: "Paid",
    totalAmount: "$250.00",
    paymentMethod: "Credit Card"
    },
    {
    invoice: "INV002",
    paymentStatus: "Pending",
    totalAmount: "$150.00",
    paymentMethod: "PayPal"
    },
    {
    invoice: "INV003",
    paymentStatus: "Unpaid",
    totalAmount: "$350.00",
    paymentMethod: "Bank Transfer"
    },
    {
    invoice: "INV004",
    paymentStatus: "Paid",
    totalAmount: "$450.00",
    paymentMethod: "Credit Card"
    },
    {
    invoice: "INV005",
    paymentStatus: "Paid",
    totalAmount: "$550.00",
    paymentMethod: "PayPal"
    },
    {
    invoice: "INV006",
    paymentStatus: "Pending",
    totalAmount: "$200.00",
    paymentMethod: "Bank Transfer"
    },
    {
    invoice: "INV007",
    paymentStatus: "Unpaid",
    totalAmount: "$300.00",
    paymentMethod: "Credit Card"
    }
  ];
</script>
<!-- <script lang="ts">
// import Table from './Table.svelte';
// import { Table } from 'shadcn-svelte';
  import * as Table from '$lib/components/ui/table';

  

</script> -->

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
      <Button variant="outline">{assessment.action}</Button>
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
 
 
<!-- <DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="ghost"
        size="icon"
        class="relative size-8 p-0"
      >
        <span class="sr-only">Open menu</span>
        <EllipsisIcon />
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content>
    <DropdownMenu.Group>
      <DropdownMenu.Label>Actions</DropdownMenu.Label>
      <DropdownMenu.Item onclick={() => navigator.clipboard.writeText(id)}>
        Copy payment ID
      </DropdownMenu.Item>
    </DropdownMenu.Group>
    <DropdownMenu.Separator />
    <DropdownMenu.Item>View customer</DropdownMenu.Item>
    <DropdownMenu.Item>View payment details</DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root> -->

<!-- <textarea bind:value={message} placeholder="Write your message"></textarea>
<p>Your message: {message}</p> -->

<!-- <DataTable data={data} {columns} /> -->

<!-- <Accordion.Root type="single">
 <Accordion.Item value="item-1">
  <Accordion.Trigger>Is it accessible?</Accordion.Trigger>
  <Accordion.Content>
   Yes. It adheres to the WAI-ARIA design pattern.
  </Accordion.Content>
 </Accordion.Item>
</Accordion.Root> -->

<!-- <Table.Root>
  <Table.Caption>A list of your recent invoices.</Table.Caption>
  <Table.Header>
    <Table.Row>
      <Table.Head class="w-[100px]">Invoice</Table.Head>
      <Table.Head>Status</Table.Head>
      <Table.Head>Method</Table.Head>
      <Table.Head class="text-end">Amount</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    <Table.Row>
      <Table.Cell class="font-medium">INV001</Table.Cell>
      <Table.Cell>Paid</Table.Cell>
      <Table.Cell>Credit Card</Table.Cell>
      <Table.Cell class="text-end">$250.00</Table.Cell>
    </Table.Row>
  </Table.Body>
</Table.Root> -->


<!-- <Alert.Root>
 <Alert.Title>Heads up!</Alert.Title>
 <Alert.Description>
  You can add components to your app using the cli.
 </Alert.Description>
</Alert.Root>


<Card.Root>
  <Card.Header>
    <Card.Title>Card Title</Card.Title>
    <Card.Description>Card Description</Card.Description>
  </Card.Header>
  <Card.Content>
    <p>Card Content</p>
  </Card.Content>
  <Card.Footer>
    <p>Card Footer</p>
  </Card.Footer>
</Card.Root>


<Badge variant="outline">Badge</Badge>


<a href="/dashboard" class={badgeVariants({ variant: "outline" })}>Badge</a> -->



</main>

<style>
textarea {
width: 100%;
height: 100px;
margin-top: 10px;
}
</style>