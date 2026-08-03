<script setup lang="ts">
import { computed } from "vue";
import { groupStyle } from "../groupColor";
import { renderNote } from "../renderNote";
import type { DocTable, Relationship, TableGroup } from "../types";
import ColumnTable from "./ColumnTable.vue";
import RelationshipList from "./RelationshipList.vue";

const props = defineProps<{
  table: DocTable;
  /** Every relationship in the snapshot; RelationshipList filters them. */
  relationships: Relationship[];
  /** The table's group, or null when it belongs to none. */
  group: TableGroup | null;
}>();

const emit = defineEmits<{
  select: [tableKey: string];
}>();

const qualified = computed(() => (props.table.schema ? `${props.table.schema}.${props.table.name}` : props.table.name));

const kindLabel = computed(() => props.table.kind.toLowerCase().replace(/_/g, " "));

/**
 * The database comment a local note replaced. Bound with `:title` so Vue
 * escapes it — it is author text, exactly like the note itself.
 */
const shadowedTitle = computed(() => (props.table.shadowedNote ? `Database comment: ${props.table.shadowedNote}` : undefined));
</script>

<template>
  <article class="flex flex-col gap-5">
    <header class="flex flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <h2 class="font-mono text-lg font-semibold text-foreground">{{ qualified }}</h2>
        <span class="rounded bg-muted/50 px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">{{ kindLabel }}</span>
        <span v-if="group" class="docs-group rounded px-1.5 py-0.5 text-[10px] font-medium" style="background-color: var(--group-tint); color: var(--group-c)" :style="groupStyle(group.hue)">
          {{ group.name }}
        </span>
        <span v-if="table.estimatedRows !== null" class="text-[10px] text-muted-foreground"> ~{{ table.estimatedRows }} rows </span>
      </div>

      <div v-if="table.note" class="flex items-start gap-2 text-sm text-muted-foreground">
        <span v-if="table.noteSource === 'LOCAL'" class="mt-0.5 shrink-0 text-[10px] font-medium" :title="shadowedTitle">⬤ LOCAL</span>
        <div v-html="renderNote(table.note)"></div>
      </div>
    </header>

    <section>
      <h3 class="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Columns</h3>
      <ColumnTable :columns="table.columns" :column-notes="table.columnNotes" />
    </section>

    <section v-if="table.indexes.length > 0">
      <h3 class="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Indexes</h3>
      <div class="overflow-hidden rounded-md border border-border">
        <table class="w-full text-xs">
          <thead>
            <tr class="bg-muted/30">
              <th class="px-2 py-1.5 text-left font-medium text-muted-foreground">Name</th>
              <th class="px-2 py-1.5 text-left font-medium text-muted-foreground">Columns</th>
              <th class="px-2 py-1.5 text-left font-medium text-muted-foreground">Settings</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="index in table.indexes" :key="index.name" class="border-t border-border align-top">
              <td class="px-2 py-1.5 font-mono">{{ index.name }}</td>
              <td class="px-2 py-1.5 font-mono text-muted-foreground">
                {{ index.columns.join(", ") }}<template v-if="index.included_columns && index.included_columns.length > 0"> (include {{ index.included_columns.join(", ") }}) </template>
              </td>
              <td class="px-2 py-1.5">
                <div class="flex flex-wrap gap-1 text-[10px] text-muted-foreground">
                  <span v-if="index.is_primary" class="rounded bg-muted/50 px-1.5 py-0.5">pk</span>
                  <span v-if="index.is_unique" class="rounded bg-muted/50 px-1.5 py-0.5">unique</span>
                  <span v-if="index.index_type" class="rounded bg-muted/50 px-1.5 py-0.5">{{ index.index_type }}</span>
                  <span v-if="index.filter" class="rounded bg-muted/50 px-1.5 py-0.5">where {{ index.filter }}</span>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <section>
      <h3 class="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Relationships</h3>
      <RelationshipList :relationships="relationships" :schema="table.schema" :table="table.name" @select="emit('select', $event)" />
    </section>

    <section v-if="table.viewDefinition">
      <h3 class="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Definition</h3>
      <pre class="overflow-x-auto rounded-md border border-border bg-muted/20 p-2 font-mono text-xs">{{ table.viewDefinition }}</pre>
    </section>
  </article>
</template>
