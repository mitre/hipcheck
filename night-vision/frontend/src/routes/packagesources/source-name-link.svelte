<script lang="ts">
 let { name, href }: { name: string; href: string } = $props();
</script>

<!-- The name links to the source too, since View source can scroll out of sight in narrow windows. -->
<a class="font-medium underline" {href}>{name}</a>
