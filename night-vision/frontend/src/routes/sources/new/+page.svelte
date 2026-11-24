<script lang="ts">
    import { enhance } from "$app/forms";
    import { Button } from "$lib/components/ui/button/index.js";
    import * as Card from "$lib/components/ui/card/index.js";
    import * as Field from "$lib/components/ui/field/index.js";
    import { Separator } from "$lib/components/ui/separator/index.js";
    import { Textarea } from "$lib/components/ui/textarea/index.js";
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import * as Alert from "$lib/components/ui/alert/index.js";
    import Info from "@lucide/svelte/icons/info";
    import CircleAlert from "@lucide/svelte/icons/circle-alert";
    import type { ActionData } from "./$types";

    let { form }: { form: ActionData } = $props();

    let selectedFile = $state<File | null>(null);
    let submitting = $state(false);

    function handleFileChange(event: Event & { currentTarget: HTMLInputElement }) {
        selectedFile = event.currentTarget.files?.[0] ?? null;
    }
</script>
<h1>Add Package Source</h1>
<p>Submit an NPM package.json to resolve reachable dependencies and find KEV-linked exposures.</p>
<br>

<form
    method="POST"
    enctype="multipart/form-data"
    use:enhance={() => {
        submitting = true;
        return async ({ update }) => {
            await update({ reset: false });
            submitting = false;
        };
    }}
>
<Card.Root>
  <Card.Header>
    <Card.Title>Source Details</Card.Title>
    <Separator />
  </Card.Header>
  <Card.Content>
    <div class="w-full max-w-md">
    <Field.Set>
        <Field.Group>
        <Tabs.Root value="paste_manifest" class="w-[400px]">
        <Tabs.List>
        <Tabs.Trigger value="paste_manifest">Paste manifest</Tabs.Trigger>
        <Tabs.Trigger value="upload_package.json">Upload package.json</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content value="paste_manifest">
            <Field.Label for="manifest">PACKAGE.JSON</Field.Label>
            <Textarea id="manifest" name="manifest" placeholder="Paste package source here..." rows={8} value={form?.manifest ?? ""} />
        </Tabs.Content>
        <Tabs.Content value="upload_package.json">
            <Field.Label for="file">PACKAGE.JSON FILE</Field.Label>
            <input id="file" name="file" type="file" accept=".json,application/json" onchange={handleFileChange} />
            {#if selectedFile}
                <p class="mt-2 text-sm">Selected: {selectedFile.name} ({selectedFile.size} bytes)</p>
            {/if}
        </Tabs.Content>
        </Tabs.Root>
        </Field.Group>
    </Field.Set>
    </div>
    <p>Night Vision accepts a valid NPM package manifest. Lockfiles and other ecosystems are outside the MVP. </p>
    {#if form?.message}
        <Alert.Root variant="destructive" class="mt-4">
            <CircleAlert />
            <Alert.Description>{form.message}</Alert.Description>
        </Alert.Root>
    {/if}
  </Card.Content>
  <Card.Footer>
    <br>
    <Alert.Root  class="bg-green-100">
    <Info />
    <Alert.Description>
        Submitting starts dependency resolution. Follow progress and review errors from the source detail page.
    </Alert.Description>
    </Alert.Root>
  </Card.Footer>
</Card.Root>

<br>
<div style="display:flex; float:right">
  <Button href="/sources" variant="outline" class="rounded-full">
    Cancel
  </Button>
  <Button type="submit" disabled={submitting} class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
    {submitting ? "Submitting…" : "Validate and Submit"}
  </Button>
</div>
</form>
