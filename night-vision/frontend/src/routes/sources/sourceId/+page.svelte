<script lang="ts">
  import * as Table from "$lib/components/ui/table/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Badge, badgeVariants } from "$lib/components/ui/badge/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
 </script>

<main>
<h2><a href="/sources">Package sources</a> / agency-web</h2>
  <h1 style="float:left">agency-web</h1>

  <Button href="/sources/new" style="float:right" class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
    View submitted manifest
  </Button>
<br>
<br>


<p>Submitted NPM manifests and their resolution status.</p>
<br>
<p> </p>
<p>NPM package.json - submitted Sep 4, 2026</p>

<Card.Root>
  <Card.Header>
    <Card.Title>Dependency resolution finished</Card.Title>
    <Separator orientation="vertical" />
        <div> Doc</div>
    <Separator orientation="vertical" />
        <div>Source</div>
  </Card.Header>
    <Card.Content>
      <div class="flex h-5 items-center gap-4 text-sm">
        <div>Blog</div>
      <Separator orientation="vertical" />
        <div>Docs</div>
      <Separator orientation="vertical" />
        <div>Source</div>
      </div>  
    <div class="w-full max-w-md">
    
    </div>
    <p>Night Vision accepts a valid NPM package manifest. Lockfiles and other ecosystems are outside the MVP. </p>
  </Card.Content>
  <Card.Footer>
    <br>
  </Card.Footer>
</Card.Root>

</main>