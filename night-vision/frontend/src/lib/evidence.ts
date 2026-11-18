const MAX_EVIDENCE_TEXT_LENGTH = 8_192;
const MAX_LINK_LENGTH = 2_048;
const TRUNCATION_MARKER = '… [truncated]';

/** Render external assessment evidence as inert, bounded plain text. */
export function displayEvidenceText(value: string, preserveNewlines = true): string {
	const normalized = value.replace(/\r\n?/g, '\n');
	let output = '';
	for (const character of normalized) {
		const displayed = escapeCharacter(character, preserveNewlines);
		if (output.length + displayed.length + TRUNCATION_MARKER.length > MAX_EVIDENCE_TEXT_LENGTH) {
			return `${output}${TRUNCATION_MARKER}`;
		}
		output += displayed;
	}
	return output;
}

/** Return a link only when the external value is a credential-free HTTPS URL. */
export function vettedEvidenceUrl(value: string): string | undefined {
	if (value.length > MAX_LINK_LENGTH) return undefined;
	try {
		const url = new URL(value);
		if (url.protocol !== 'https:' || url.username || url.password || !url.hostname) return undefined;
		return url.toString();
	} catch {
		return undefined;
	}
}

/** Return a safe visible URL label without disclosing embedded credentials. */
export function displayEvidenceUrlLabel(value: string): string {
	try {
		const url = new URL(value);
		if (url.username || url.password) {
			url.username = '<redacted>';
			url.password = '<redacted>';
		}
		return displayEvidenceText(url.toString(), false);
	} catch {
		return displayEvidenceText(redactUnparsedUrlAuthentication(value), false);
	}
}

function escapeCharacter(character: string, preserveNewlines: boolean): string {
	if (character === '\n' && preserveNewlines) return '\n';
	if (character === '\n') return '\\n';
	if (character === '\t') return '\\t';
	if (isControl(character) || isBidiControl(character)) {
		return `\\u{${character.codePointAt(0)?.toString(16).padStart(4, '0')}}`;
	}
	return character;
}

function isControl(character: string): boolean {
	const point = character.codePointAt(0) ?? 0;
	return point <= 0x1f || (point >= 0x7f && point <= 0x9f);
}

function isBidiControl(character: string): boolean {
	const point = character.codePointAt(0) ?? 0;
	return (
		point === 0x061c ||
		point === 0x200e ||
		point === 0x200f ||
		(point >= 0x202a && point <= 0x202e) ||
		(point >= 0x2066 && point <= 0x2069)
	);
}

function redactUnparsedUrlAuthentication(value: string): string {
	const schemeEnd = value.indexOf('://');
	if (schemeEnd === -1) return value;
	const prefix = value.slice(0, schemeEnd + 3);
	const remainder = value.slice(schemeEnd + 3);
	const authorityEnd = remainder.search(/[/?#]/);
	const authority = authorityEnd === -1 ? remainder : remainder.slice(0, authorityEnd);
	const suffix = authorityEnd === -1 ? '' : remainder.slice(authorityEnd);
	const at = authority.lastIndexOf('@');
	return at === -1 ? value : `${prefix}<redacted>@${authority.slice(at + 1)}${suffix}`;
}
