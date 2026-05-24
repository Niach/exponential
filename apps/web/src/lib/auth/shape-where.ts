export function sqlStringLiteral(value: string): string {
  return `'${value.replace(/'/g, `''`)}'`
}

export function buildWhereClause(column: string, ids: string[]): string {
  if (ids.length === 0) {
    return `"${column}" = ${sqlStringLiteral(`00000000-0000-0000-0000-000000000000`)}`
  }

  const escapedIds = ids.map(sqlStringLiteral).join(`,`)
  return `"${column}" IN (${escapedIds})`
}
