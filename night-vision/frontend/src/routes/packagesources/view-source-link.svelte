<script lang="ts">
 import { Button } from "$lib/components/ui/button/index.js";

 let { href }: { href: string } = $props();
</script>

<Button {href} variant="outline" size="sm">View source</Button>
