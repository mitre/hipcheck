<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";

  const candidates = [
    {
    candidate: "4.17.21",
    known_exposure: "No longer affected",
    action: "Selected",
    },
    {
    candidate: "4.17.22",
    known_exposure: "Evidence unavailable",
    action: "Choose",
    }
  ]
 </script>

<main>
<h2 style="color:grey;"><a href="/assessments">Assessments</a> / NV-2026-0142</h2>
<h1 style="font-weight:bold;">Is this patch a reasonable upgrade?</h1>
<p>Follow the evidence from a KEV-linked dependency to a cautious recommendation. </p>
<br>
<br>

<p style="color: green; font-weight: bold;">
    <Badge class="bg-green-100 text-green-800 dark:bg-green-950 dark:text-green-300">✓</Badge>
    Exposure found ⸻⸻
    <Badge class="bg-green-100 text-green-800 dark:bg-green-950 dark:text-green-300">✓</Badge>
    Candidate chosen ⸻⸻
    <Badge class="bg-green-100 text-green-800 dark:bg-green-950 dark:text-green-300">✓</Badge>
    Risk checked 
    <text style="color:grey;">⸻⸻</text>
    <Badge variant="outline">4</Badge>
    <text style="color: black;">Verdict</text>
</p>
<br>

<!-- Patch step 1 -->
<Card.Root>
  <Card.Header>
    <Card.Title>
        <p style="color: black; display:flex; float:left; font-weight: bold;">
            <Badge class="bg-blue-100 text-mist-600 dark:bg-blue-950 dark:text-blue-300">1</Badge>
            Start with the reachable exposure
        </p>
        <p style="color: black; font-size: 13px; display:flex; float:right">
            from agency-web/package.json
        </p>
    </Card.Title>
  </Card.Header>
    <Card.Content>
      <p style="font-weight: bold; font-size:20px;">lodash@4.17.20 <Badge variant="destructive">KEV affected</Badge></p>
      <p>Reachable through api-client · CVE-2021-23337 is a CISA Known Exploited Vulnerability.</p>
  </Card.Content>
</Card.Root>
<br>

<!-- Patch step 2 -->
<Card.Root>
  <Card.Header>
    <Card.Title>
        <p style="color: black; display:flex; float:left; font-weight: bold;">
            <Badge class="bg-blue-100 text-mist-600 dark:bg-blue-950 dark:text-blue-300">2</Badge>
            Choose a newer patch candidate
        </p>
        <p style="color: black; font-size: 13px; display:flex; float:right">
            Patch upgrades only
        </p>
    </Card.Title>
  </Card.Header>

    <Card.Content>
      <Table.Root>
        <Table.Header>
        <Table.Row style="font-size: 13px; color:slategrey;">
        <Table.Head>CANDIDATE</Table.Head>
        <Table.Head>KNOWN EXPOSURE</Table.Head>
        <Table.Head class="text-end">SELECT</Table.Head>
        </Table.Row>
        </Table.Header>
        <Table.Body>
        {#each candidates as candidate (candidate)}
        <Table.Row>
            <Table.Cell>
            <p style="font-weight: bold;">{candidate.candidate}</p>
            </Table.Cell>
            <Table.Cell>
                {#if candidate.known_exposure == "No longer affected"}
                <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">No longer affected</Badge>
                {:else if candidate.known_exposure == "Evidence unavailable"}
                <Badge class="bg-amber-100 text-amber-800 dark:bg-blue-950 dark:text-blue-300">Evidence unavailable</Badge>
                {:else}
                <Badge variant="destructive">Unknown</Badge>
                {/if}
            </Table.Cell>
            <Table.Cell class="text-end">
                {#if candidate.action == "Selected"}
                <Button variant="secondary">{candidate.action}</Button>
                {:else}
                <Button variant="outline">{candidate.action}</Button>
                {/if}
            </Table.Cell>
        </Table.Row>
        {/each}
        </Table.Body>
      </Table.Root>
  </Card.Content>
</Card.Root>
<br>

<!-- Patch step 3 -->
<Card.Root>
  <Card.Header>
    <Card.Title>
        <p style="color: black; display:flex; float:left; font-weight: bold;">
          <Badge class="bg-blue-100 text-mist-600 dark:bg-blue-950 dark:text-blue-300">3</Badge>
          Check the selected release
        </p>
        <p style="color: black; font-size: 13px; display:flex; float:right">
          Candidate 4.17.21
        </p>
    </Card.Title>
  </Card.Header>
    <Card.Content>
      <div class="flex h-5 items-center gap-4 text-sm">
        <div>
            <p style="font-size: 10px;">VULNERABILITY STATUS</p>
            <p>✓ Ouside affected range</p>
        </div>
      <Separator orientation="vertical" />
        <div>
            <p style="font-size: 10px;">SUPPLY-CHAIN SIGNALS</p>
            <p>✓ No blocking findings</p>
        </div>
      <Separator orientation="vertical" />
        <div>
            <p style="font-size: 10px;">EVIDENCE QUALITY</p>
            <p>✓ Sources agree</p>
        </div>
      </div>  
  </Card.Content>
</Card.Root>
<br>

<!-- Patch step 4 -->
<Card.Root>
  <Card.Header>
    <Card.Title>
        <p style="color: black; display:flex; float:left; font-weight: bold;">
            <Badge class="bg-blue-100 text-mist-600 dark:bg-blue-950 dark:text-blue-300">4</Badge>
            Make the recommendation
        </p>
        <p style="color: black; font-size: 13px; display:flex; float:right">
            Based on the checks above
        </p>
    </Card.Title>
  </Card.Header>
    <Card.Content>
      <p style="font-weight: bold; font-size:20px;">Recommended</p>
      <p>4.17.21 appears to remove the KEV-linked exposure; completed checks found no serious signal.</p>
      <p>
        <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">✓ Issue appears fixed</Badge>
        <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">✓ No blocking signal</Badge>
        <Badge class="bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300">✓ Patch-level upgrade</Badge>
      </p>
    </Card.Content>
</Card.Root>
</main>