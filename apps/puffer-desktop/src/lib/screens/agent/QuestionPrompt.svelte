<script lang="ts">
  import Icon from "../../design/Icon.svelte";
  import type { AskUserQuestionItem, UserQuestionTimelineItem } from "../../types";

  type Answers = Record<string, string | string[]>;
  type Annotations = Record<string, Record<string, string>>;

  type Props = {
    item: UserQuestionTimelineItem;
    onResolve: (questionId: string, answers: Answers, annotations?: Annotations) => void;
  };

  let { item, onResolve }: Props = $props();
  let answers = $state<Answers>({});
  let otherText = $state<Record<string, string>>({});

  function valueFor(question: AskUserQuestionItem): string | string[] {
    return answers[question.question] ?? (question.multiSelect ? [] : "");
  }

  function setSingle(question: AskUserQuestionItem, label: string) {
    answers = { ...answers, [question.question]: label };
    if (item.questions.length === 1) submit({ ...answers, [question.question]: label });
  }

  function toggleMulti(question: AskUserQuestionItem, label: string) {
    const current = valueFor(question);
    const list = Array.isArray(current) ? current : [];
    const next = list.includes(label) ? list.filter((v) => v !== label) : [...list, label];
    answers = { ...answers, [question.question]: next };
  }

  function setOther(question: AskUserQuestionItem, value: string) {
    const previous = otherText[question.question];
    otherText = { ...otherText, [question.question]: value };
    if (!question.multiSelect) {
      answers = { ...answers, [question.question]: value };
      return;
    }
    const current = valueFor(question);
    const base = Array.isArray(current)
      ? current.filter((entry) => !previous || entry !== previous)
      : [];
    answers = {
      ...answers,
      [question.question]: value.trim() ? [...base, value] : base
    };
  }

  function checked(question: AskUserQuestionItem, label: string): boolean {
    const current = valueFor(question);
    return Array.isArray(current) ? current.includes(label) : current === label;
  }

  function hasAnswer(question: AskUserQuestionItem, source: Answers = answers): boolean {
    const current = source[question.question];
    if (Array.isArray(current)) return current.length > 0;
    return typeof current === "string" && current.trim().length > 0;
  }

  function canSubmit(source: Answers = answers): boolean {
    return item.questions.every((question) => hasAnswer(question, source));
  }

  function submit(source: Answers = answers) {
    if (!canSubmit(source)) return;
    onResolve(item.id, source, {});
  }
</script>

<div class="pf-question">
  <div class="pf-question-head">
    <Icon name="sparkles" size={14} color="var(--puffer-accent)" />
    Question
  </div>
  {#each item.questions as question (question.question)}
    <div class="pf-question-block">
      <div class="pf-question-kicker">{question.header}</div>
      <div class="pf-question-title">{question.question}</div>
      <div class="pf-question-options" data-multi={question.multiSelect === true}>
        {#each question.options as option (option.label)}
          <button
            type="button"
            class="pf-question-option"
            data-selected={checked(question, option.label)}
            onclick={() =>
              question.multiSelect
                ? toggleMulti(question, option.label)
                : setSingle(question, option.label)}
          >
            <span>{option.label}</span>
            <small>{option.description}</small>
          </button>
        {/each}
      </div>
      <div class="pf-question-other">
        <input
          value={otherText[question.question] ?? ""}
          placeholder="Other answer"
          oninput={(event) =>
            setOther(question, (event.currentTarget as HTMLInputElement).value)}
        />
      </div>
    </div>
  {/each}
  <div class="pf-question-actions">
    <button
      type="button"
      class="sc-btn"
      data-variant="default"
      data-size="sm"
      disabled={!canSubmit()}
      onclick={() => submit()}
    >
      Send answer
    </button>
  </div>
</div>

<style>
  .pf-question {
    border: 1px solid color-mix(in oklab, var(--puffer-accent) 42%, var(--border));
    background: color-mix(in oklab, var(--puffer-accent) 5%, var(--background));
    border-radius: 10px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 13px;
  }

  .pf-question-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    font-weight: 600;
  }

  .pf-question-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .pf-question-kicker {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 11px;
    text-transform: uppercase;
  }

  .pf-question-title {
    color: var(--foreground);
    font-weight: 600;
  }

  .pf-question-options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 8px;
  }

  .pf-question-option {
    border: 1px solid var(--border);
    background: var(--background);
    color: var(--foreground);
    border-radius: 8px;
    padding: 9px 10px;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 3px;
    cursor: pointer;
  }

  .pf-question-option[data-selected="true"] {
    border-color: var(--puffer-accent);
    background: color-mix(in oklab, var(--puffer-accent) 10%, var(--background));
  }

  .pf-question-option span {
    font-weight: 600;
  }

  .pf-question-option small {
    color: var(--muted-foreground);
    line-height: 1.35;
  }

  .pf-question-other input {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--background);
    color: var(--foreground);
    padding: 8px 10px;
    font: inherit;
  }

  .pf-question-actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
