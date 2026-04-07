<script lang="ts">
  type InlineSegment = {
    kind: "text" | "code";
    text: string;
  };

  type MessageBlock =
    | { kind: "paragraph"; text: string }
    | { kind: "list"; ordered: boolean; items: string[] }
    | { kind: "quote"; text: string }
    | { kind: "code"; language: string | null; text: string };

  export let body = "";

  function inlineSegments(text: string): InlineSegment[] {
    const parts: InlineSegment[] = [];
    const pattern = /`([^`]+)`/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = pattern.exec(text)) !== null) {
      if (match.index > lastIndex) {
        parts.push({ kind: "text", text: text.slice(lastIndex, match.index) });
      }
      parts.push({ kind: "code", text: match[1] });
      lastIndex = match.index + match[0].length;
    }

    if (lastIndex < text.length) {
      parts.push({ kind: "text", text: text.slice(lastIndex) });
    }

    return parts.length > 0 ? parts : [{ kind: "text", text }];
  }

  function parseBlocks(source: string): MessageBlock[] {
    const blocks: MessageBlock[] = [];
    const lines = source.replace(/\r\n?/g, "\n").split("\n");
    let paragraphLines: string[] = [];
    let quoteLines: string[] = [];
    let listItems: string[] = [];
    let listOrdered = false;

    function flushParagraph() {
      if (paragraphLines.length === 0) {
        return;
      }
      blocks.push({
        kind: "paragraph",
        text: paragraphLines.join(" ").trim()
      });
      paragraphLines = [];
    }

    function flushQuote() {
      if (quoteLines.length === 0) {
        return;
      }
      blocks.push({
        kind: "quote",
        text: quoteLines.join("\n").trim()
      });
      quoteLines = [];
    }

    function flushList() {
      if (listItems.length === 0) {
        return;
      }
      blocks.push({
        kind: "list",
        ordered: listOrdered,
        items: [...listItems]
      });
      listItems = [];
    }

    for (let index = 0; index < lines.length; index += 1) {
      const line = lines[index];
      const codeFence = line.match(/^```([\w-]+)?\s*$/);
      if (codeFence) {
        flushParagraph();
        flushQuote();
        flushList();
        const codeLines: string[] = [];
        let innerIndex = index + 1;
        while (innerIndex < lines.length && !lines[innerIndex].startsWith("```")) {
          codeLines.push(lines[innerIndex]);
          innerIndex += 1;
        }
        blocks.push({
          kind: "code",
          language: codeFence[1] ?? null,
          text: codeLines.join("\n")
        });
        index = innerIndex;
        continue;
      }

      if (line.trim() === "") {
        flushParagraph();
        flushQuote();
        flushList();
        continue;
      }

      const orderedItem = line.match(/^\d+\.\s+(.*)$/);
      const unorderedItem = line.match(/^[-*]\s+(.*)$/);
      if (orderedItem || unorderedItem) {
        flushParagraph();
        flushQuote();
        const ordered = Boolean(orderedItem);
        const text = (orderedItem?.[1] ?? unorderedItem?.[1] ?? "").trim();
        if (listItems.length > 0 && ordered !== listOrdered) {
          flushList();
        }
        listOrdered = ordered;
        listItems.push(text);
        continue;
      }

      if (line.startsWith("> ")) {
        flushParagraph();
        flushList();
        quoteLines.push(line.slice(2));
        continue;
      }

      flushQuote();
      paragraphLines.push(line.trim());
    }

    flushParagraph();
    flushQuote();
    flushList();

    return blocks;
  }

  $: blocks = parseBlocks(body);
</script>

<div class="message-body">
  {#each blocks as block}
    {#if block.kind === "paragraph"}
      <p>
        {#each inlineSegments(block.text) as segment}
          {#if segment.kind === "code"}
            <code>{segment.text}</code>
          {:else}
            {segment.text}
          {/if}
        {/each}
      </p>
    {:else if block.kind === "list"}
      <svelte:element this={block.ordered ? "ol" : "ul"} class="list">
        {#each block.items as item}
          <li>
            {#each inlineSegments(item) as segment}
              {#if segment.kind === "code"}
                <code>{segment.text}</code>
              {:else}
                {segment.text}
              {/if}
            {/each}
          </li>
        {/each}
      </svelte:element>
    {:else if block.kind === "quote"}
      <blockquote>{block.text}</blockquote>
    {:else}
      <div class="code-block">
        {#if block.language}
          <span class="language">{block.language}</span>
        {/if}
        <pre>{block.text}</pre>
      </div>
    {/if}
  {/each}
</div>

<style>
  .message-body {
    display: grid;
    gap: 0.85rem;
  }

  p,
  blockquote,
  pre {
    margin: 0;
  }

  p,
  li,
  blockquote {
    line-height: 1.72;
  }

  .list {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.35rem;
  }

  blockquote {
    padding: 0.8rem 0.95rem;
    border-left: 3px solid rgba(20, 99, 86, 0.24);
    background: rgba(222, 238, 232, 0.38);
    border-radius: 14px;
    color: var(--text-muted);
    white-space: pre-wrap;
  }

  code {
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.92em;
    padding: 0.08rem 0.32rem;
    border-radius: 8px;
    background: rgba(247, 243, 235, 0.92);
    border: 1px solid rgba(111, 101, 89, 0.12);
  }

  .code-block {
    display: grid;
    gap: 0.45rem;
  }

  .language {
    color: var(--text-muted);
    font-size: 0.74rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  pre {
    padding: 0.85rem 0.95rem;
    border-radius: 16px;
    background: rgba(247, 243, 235, 0.82);
    border: 1px solid rgba(111, 101, 89, 0.14);
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.82rem;
    line-height: 1.58;
    white-space: pre-wrap;
    overflow: auto;
  }
</style>
