<script>
    import { Button } from "$lib/components/ui/button/index.js";
    import * as Card from "$lib/components/ui/card/index.js";
    import * as Field from "$lib/components/ui/field/index.js";
    import { Input } from "$lib/components/ui/input/index.js";
    import { Separator } from "$lib/components/ui/separator/index.js";
    import { Textarea } from "$lib/components/ui/textarea/index.js";
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import * as Alert from "$lib/components/ui/alert/index.js";
    import Info from "@lucide/svelte/icons/info";


        let files = [];

    // Optional: preview file names before upload
    function handleFileChange(event) {
        files = Array.from(event.target.files);
    }

    async function uploadFiles() {
        if (!files.length) {
            alert("Please select at least one file.");
            return;
        }

        const formData = new FormData();
        files.forEach(file => formData.append("files", file));

        try {
            const res = await fetch("/upload", {
                method: "POST",
                body: formData
            });

            if (!res.ok) throw new Error(await res.text());
            const data = await res.json();
            alert(`Uploaded: ${data.uploaded.join(", ")}`);
        } catch (err) {
            console.error(err);
            alert("Upload failed.");
        }
    }

</script>
<h1>Add Package Source</h1>
<p>Submit a NPM package.json to resolve reachable dependencies and find KEV-linked exposures.</p>
<br>

<Card.Root>
  <Card.Header>
    <Card.Title>Source Details</Card.Title>
    <Separator />
  </Card.Header>
  <Card.Content>
    <div class="w-full max-w-md">
    <Field.Set>
        <Field.Group>
        <Field.Field>
            <Field.Label for="source_name">SOURCE NAME</Field.Label>
            <Input id="username" type="text" placeholder="Input source name here"/>
            <Field.Description>
                Use a short name that identifies this application or repository.
            </Field.Description>
        </Field.Field>

        <Tabs.Root value="paste_manifest" class="w-[400px]">
        <Tabs.List>
        <Tabs.Trigger value="paste_manifest">Paste manifest</Tabs.Trigger>
        <Tabs.Trigger value="upload_package.json">Upload package.json</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content value="paste_manifest">
            <Field.Label for="feedback">PACKAGE.JSON</Field.Label>
        <Textarea id="feedback" placeholder="Paste package source here..." rows={8}/>
        </Tabs.Content>
        <Tabs.Content value="upload_package.json">Upload json files:
        <input type="file" multiple on:change={handleFileChange} />
        <button on:click={uploadFiles}>Upload</button>
        <ul>
            {#each files as file}
                <li>{file.name}</li>
            {/each}
        </ul>
        {#if files}
            <p>Selected files:</p>
            {#each Array.from(files) as file}
                <p>{file.name} ({file.size} bytes)</p>
            {/each}
        {/if}
        </Tabs.Content>
        </Tabs.Root>
        
        </Field.Group>
    </Field.Set>
    </div>
    <p>Night Vision accepts a valid NPM package manifest. Lockfiles and other ecosystems are outside the MVP. </p>
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
  <Button href="/sources" class="rounded-full bg-cyan-900 text-gray-200 dark:bg-gray-600 dark:text-gray-50">
    Validate and Submit
  </Button>
</div>

