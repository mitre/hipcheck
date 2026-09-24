import { fail, redirect } from '@sveltejs/kit';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { Actions } from './$types';

export const actions: Actions = {
	default: async ({ request }) => {
		const form = await request.formData();
		const file = form.get('file');
		const pasted = form.get('manifest');
		const upload = file instanceof File && file.size > 0 ? file : null;
		const manifest = typeof pasted === 'string' ? pasted : '';
		const contents = upload ? await upload.text() : manifest.trim();

		if (!contents) {
			return fail(400, { message: 'Paste a package.json or choose a file to upload.', manifest });
		}

		// The API accepts npm manifests under their standard name, whatever the uploaded file was called.
		const result = await nightVisionApi.submitPackageSource({ fileName: 'package.json', contents });
		if (!result.ok) {
			// A 400 here can only mean the manifest itself was rejected.
			if (result.error.status === 400) {
				return fail(400, {
					message: 'Night Vision could not read that as a package.json. Check that it is valid JSON for one npm manifest, under 1 MiB.',
					manifest
				});
			}
			return fail(502, { message: result.error.message, manifest });
		}

		redirect(303, `/packagesources/${result.data.id}`);
	}
};
