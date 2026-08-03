<script setup lang="ts">
import type { IndexSection } from "../docsIndex";
import { groupStyle } from "../groupColor";
import { renderNote } from "../renderNote";
import type { DocTable } from "../types";

defineProps<{
  sections: IndexSection[];
}>();

const emit = defineEmits<{
  select: [tableKey: string];
}>();

function tableKey(table: DocTable): string {
  return table.schema ? `${table.schema}.${table.name}` : table.name;
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <section v-for="section in sections" :key="section.key" class="flex flex-col gap-2">
      <div class="flex flex-col gap-1 border-l-2 pl-3" :class="{ 'docs-group': section.hue !== null }" style="border-color: var(--group-c, var(--border))" :style="groupStyle(section.hue)">
        <div class="flex items-baseline gap-2">
          <h2 class="text-sm font-semibold text-foreground">{{ section.label || "(no schema)" }}</h2>
          <span class="text-xs text-muted-foreground">{{ section.tables.length }} tables</span>
        </div>
        <div v-if="section.note" class="text-xs text-muted-foreground" v-html="renderNote(section.note)"></div>
      </div>

      <ul class="grid gap-1 sm:grid-cols-2 lg:grid-cols-3">
        <li v-for="table in section.tables" :key="tableKey(table)">
          <button type="button" class="w-full rounded border border-border bg-background px-2 py-1.5 text-left transition-colors hover:bg-muted/40" @click="emit('select', tableKey(table))">
            <div class="flex items-baseline gap-1.5">
              <span class="font-mono text-xs font-medium text-foreground">{{ table.name }}</span>
              <span v-if="table.kind !== 'TABLE'" class="text-[10px] uppercase text-muted-foreground">
                {{ table.kind.toLowerCase().replace(/_/g, " ") }}
              </span>
            </div>
            <div v-if="table.note" class="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground" v-html="renderNote(table.note)"></div>
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>
