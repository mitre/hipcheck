
# AI Policy

Large Language Model-based tools (hereafter just called "AI") are a powerful
and controversial development in the field of software engineering. It's
important as we employ them to help develop Night Vision that we do so in a way
which maintains rigor in our software engineering practice and shows respect
for the time and attention of our coworkers.

The following document lays out rules for using AI on Night Vision, along with
a longer values-based explanation of how to think about AI on the team.

We may add to or adjust these rules over time based on experience. As they
stand, they are a synthesis of two key sources:

- [Oxide's RFD 576, "Using LLMs at Oxide"](https://rfd.shared.oxide.computer/rfd/0576)
- [Kubernetes' "AI Guidance" for OSS contributors](https://www.kubernetes.dev/docs/guide/pull-requests/#ai-guidance)

## Table of Contents

[[_TOC_]]

## Terminology

First, quickly, let's lay out some definitions:

<dl>
  <dt>AI</dt>
  <dd>General term for a system based on a Large Language Model (LLM).</dd>
  <dt>Large Language Model (LLM)</dt>
  <dd>Type of machine learning model trained on large amounts of text.</dd>
  <dt>AI Model (or just "Model")</dt>
  <dd>The particular LLM being employed. For example: Claude Opus 4.6 (by Anthropic), GPT-5.5 (by OpenAI), Llama 3.1 (by Meta, formerly Facebook).</dd>
  <dt>Harness</dt>
  <dd>Specialized software built around an AI model that turns it into a usable tool for some set of tasks.</dd>
  <dt>AI Agent</dt>
  <dd>An AI model plus a harness that enables the AI to perform tasks autonomously as directed by the user (writing code, gathering and summarizing materials, etc.).</dd>
  <dt>Slop</dt>
  <dd>LLM-generated material, created and used without care for quality.</dd>
  <dt>LLM Sycophancy</dt>
  <dd>The tendency of LLMs to behave sycophantically toward their users, effusively praising them or being dangerously uncritical.</dd>
</dl>

## The Rules

1. You can use AI tools to write code for Night Vision, but you are responsible
   for anything you submit. That means you need to review the code, test it,
   etc., just as you would for any contribution you wrote by hand. Please
   respect the other contributors and do not waste their time submitting code
   you haven't read or can't justify or explain.
2. Disclose in Merge Requests when AI has been used to help prepare changes,
   and explain the nature of how it was used. This is intended to raise
   teammates' awareness so they can be appropriately cautious about errors
   which LLMs tend to introduce.
3. Do not use commit trailers such as "Co-Authored-By" or "Assisted-By" when
   using AI tools. These are tools, not co-authors, and we don't have a
   practice of disclosing tools used in the prepation of a Merge Request within
   commit messages.
4. Do not use AI to generate replies to your coworkers in discussions. We talk
   to each other, not to chatbots.

## AI, Rigor, and Respect

When using AI on Night Vision it's important to assess your use against two
key values: rigor and respect.

For rigor: we care about delivering a system we understand; one which carefully
handles the possible states it may enter and inputs it may receive, and which
is graceful under load and precise when reporting or handling errors. We are
_not building slop_, and should not accept slop in the construction of our
software. Insofar as we use AI, we should use it consistent with this goal to
create a system which is resilient, maintainable, debuggable, and performant.

For respect: we are a _team_, and we care about and support each other.
Everyone on the team has expertise and a perspective worth hearing, and we
should endeavor to bring our collective intelligence to bear in solving
problems. When we collaborate, we work person-to-person, and do not
substitute AI chatbots for ourselves. If we do want to incorporate the output
of an AI system in a discussion with coworkers, we should clearly signpost that
we're sharing the results of an AI prompt, so that we as a team can apply our
collective sensibilities to assess the accuracy and validity of its output.
We should never mislead each other about what material is or is not
AI-generated.

## Good and Bad Times to Use AI

It's important to understand, as you contemplate leveraging AI in your
contributions, what the right times are to do so. AI systems have strengths and
weaknesses, and use of them should take these into account.

### For Generating Stuff

AI systems are at their best when their output can be tested or validated. As
such, using AI for code, for which you can enforce typechecking or test passage
(the ideal), or validate manually via sample executions (not as preferable, but
acceptable), is the ideal case. Using AI for prose, especially prose with
substantial factual components, is more difficult and should only be done
carefully. AI systems can be prone to hallucinations — the invention of false
information or material — and all factual claims or references in AI-generated
prose must be checked for these hallucinations. As such, we recommend only
using AI for prose generation in cases where you as the user have sufficient
expertise to check the output.

AI is very good at generating boilerplate code. Code which may take substantial
time to write but which is of minimal complexity is an ideal case for AI
generation in software. In Rust for example, it's common to create a type
which is conceptually simple but which needs to, for full utility, implement a
large collection of common traits from the standard library. These
implementations are often uninteresting but can represent a substantial amount
of code. Consider using AI for this kind of code, and then be sure to validate
the output to ensure the generated code performs the expected operations
correctly.

AI is less good at handling complex code, which may be responsible for managing
a large amount of complex state, or where ordering of operations may be
especially sensitive. When using AI to generate code in these cases, take extra
time to validate the output, likely with extensive testing both to ensure
correctness and to protect against future regression.

### For Reviewing Stuff

AI can be very useful as a reviewer! When doing code review, whether for your
own code before you submit it in a Merge Request, or in code submitting in a
Merge Request by a team member, you can often get useful findings by asking
an AI tool for help.

For prose, it can also be useful to have an LLM review text you've written,
late in the editing process, to get feedback on the power of your argumentation,
the flow and effectiveness of the words themselves, and the overall structure
of a piece. Its feedback will not always be worth taking, but it can be a
useful sounding board.

When using AI as an editor, beware the lure of sycophancy. AI systems can be
sycophantic (to varying degrees by model, perhaps modulated by custom prompts
you've provided), and you should not mistake the effusive praise of an AI
system as proof of correctness or evidence of your own virtue or value.

Do not engage AI editors too early in the editing process. The earlier you are,
the more likely they are to attempt to steer the arguments themselves, perhaps
pushing you toward alternative positions, rather than helping to polish or
strengthen the presentation of the perspective you actually hold.

### For Debugging Stuff

AI systems can be very useful as an aid when debugging. You can often get
helpful hints (or, in some cases, even near-complete solutions) when debugging
compiler errors or failing tests with the assistance of AI tools. In these
situations, always follow up on the suggestions given by the tools; do not
take them at face value as whole-cloth solutions.

## Using AI Effectively

It's a reality of using AI tools that the quality of your prompts
*really matters* for the quality of the output. Prompting AI systems effectively
is a skill, and one that can be learned and developed over time. The specifics
of how to effectively prompt will vary depending on the specific model in use.
We highly recommend seeking out guides to effective prompting for whatever
model you're using. Some examples:

- [OpenAI's GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)
- [Anthropic's Claude Prompt Engineering Best Practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- [Google's Gemini 3 Prompting Guide](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/start/gemini-3-prompting-guide)

Pay attention to the model you're using to ensure that any guide you try is
actually applicable to that model. There can be substantial differences from
version to version, so a guide for GPT-5.4 might not apply well to GPT-5.5.

## Risk Mitigation

There are specific risks when using AI tools, especially AI agents, that are
essential to understand and mitigate.

First, prompt injection is a real issue. Prompt injection is when attackers
are able to provide specially-crafted inputs to an AI system that causes it to
misbehave in a way intended by the attacker. For example, an AI agent in
possession of an API token from its user may suffer a prompt injection which
causes it to misuse that API token or disclose it to the attacker.

You can think of prompt injection a little bit like a social engineering attack
against an employee with access to company systems. Just as a cyber criminal
may send a phishing text to an executive secretary, impersonating the CEO and
demanding the secretary immediately send a large amount of money to some
account for an urgent business reason; so too may an LLM attacker feed an LLM
text that says to give them an API token urgently.

For this reason, __lock down all API tokens which are accessible to an AI
agent__. This includes not just tokens which you have explicitly provided to
an AI agent, but also ones which may be present on a filesystem to which the
agent has access (for example, sitting in a `.env` file). Anything the agent
can access has the potential to be exposed to an attacker via prompt injection.

Second, LLM systems may take destructive actions autonomously. While these AI's
system prompts (or any custom prompts you've provided) may tell them to ask
for permission before taking destructive actions, those protections can fail.
Do not give an AI system unfettered access to production systems. Instead,
consider constraints where AI can recommend changes which must be accepted or
rejected by a human operator.

Third, AI's coherence and controls will degrade as the length of a conversation
extends to infinity. AI models have a limited "context window" (the length
of "tokens" they can take as input). That context window is made to include 1)
the "system prompt" (an always-present prompt provided by the model's
creator), 2) any persistant user prompts you've configured, 3) the history of
messages sent in a conversation, and 4) your actual current prompt (the
specific message you've sent just now). One of the jobs of AI harnesses is to
periodically compress the context window to enable conversations to continue
as they approach the context window size limit; but this compression inherently
risks loss of context, including safety controls, from earlier in the
conversation. For this reason, be wary of long-running conversations, and
instead keep conversations focused on specific topics, and create new
conversations for new topics. If you want to provide persistent context for
the AI, consider either adding it to the Night Vision product docs and then
pointing your AI agent at those docs, or creating reusable local templates
for yourself which include that context.

## Conclusion

There is of course much more that could be said about using AI systems
effectively, and since this remains a relatively new technology we are still
collectively figuring out the most effective mechanisms for using them. This
document will continue to evolve as we on the Night Vision team figure out our
own pain points with AI systems.

If you have any thoughts or recommended changes for this document, please raise
them with the team or open a Merge Request! These rules and the accompanying
guide are a team project.
