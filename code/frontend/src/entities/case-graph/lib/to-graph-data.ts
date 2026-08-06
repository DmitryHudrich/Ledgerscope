import type { EdgeDisplaySettings, GraphData, GraphEdge, GraphNode } from '@/shared/graph'
import { DEFAULT_EDGE_DISPLAY, colorForSymbol } from '@/shared/graph'
import { shortAddress } from '@/shared/lib/format'
import { edgeKey, type CaseGraphEdge, type CaseGraphNode } from '@/entities/case-graph/model/graph'
import { nodeGroup } from '@/entities/case-graph/lib/risk'
import { COMPLETENESS_BORDER, nodeCompleteness } from '@/entities/case-graph/lib/completeness'

export type BuildGraphOptions = {
  /** Custom labels keyed by lowercased address; win over service names. */
  labelOverrides?: ReadonlyMap<string, string>
  /** Edge rendering preferences. Defaults to {@link DEFAULT_EDGE_DISPLAY}. */
  edgeDisplay?: EdgeDisplaySettings
}

function edgeAmount(edge: CaseGraphEdge): number {
  const value = Number(edge.formatted)
  return Number.isFinite(value) ? value : 0
}

function weightForAmount(amount: number): number {
  return Math.max(1, Math.min(10, Math.log10(amount + 1) * 2.2))
}

/** Compact display of a summed amount, tuned for edge labels. */
function formatAmount(value: number): string {
  if (!Number.isFinite(value) || value === 0) {
    return '0'
  }
  const digits = value >= 1000 ? 0 : value >= 1 ? 2 : 6
  return value.toLocaleString(undefined, { maximumFractionDigits: digits })
}

/**
 * Projects the accumulated engine nodes/edges onto the shared graph contract.
 *
 * Nodes render their service/custom label *in front of* the shortened address
 * (`Binance · 0x12…ab`) so an analyst reads the entity before the hash. Edges
 * are shaped by {@link EdgeDisplaySettings}: optional per-currency colour,
 * amount-scaled width, and either one arc per transfer or a single aggregated
 * arc carrying the transfer count.
 */
export function buildGraphData(
  nodes: CaseGraphNode[],
  edges: CaseGraphEdge[],
  options: BuildGraphOptions = {},
): GraphData {
  const { labelOverrides, edgeDisplay = DEFAULT_EDGE_DISPLAY } = options
  const nodeByAddress = new Map(nodes.map((node) => [node.address.toLowerCase(), node]))
  const degree = new Map<string, number>()
  const addresses = new Set<string>(nodeByAddress.keys())

  for (const edge of edges) {
    const from = edge.from.toLowerCase()
    const to = edge.to.toLowerCase()
    addresses.add(from)
    addresses.add(to)
    degree.set(from, (degree.get(from) ?? 0) + 1)
    degree.set(to, (degree.get(to) ?? 0) + 1)
  }

  const graphNodes: GraphNode[] = Array.from(addresses).map((address) => {
    const node = nodeByAddress.get(address)
    const custom = labelOverrides?.get(address)?.trim()
    const serviceName = node?.serviceName?.trim()
    const short = shortAddress(address)
    // Label (custom > service) goes first, then the address, so an entity is
    // recognisable at a glance while the hash stays visible for reference.
    const named = custom || serviceName
    const label = named ? `${named} · ${short}` : short
    const rawWeight = node?.txCount ?? degree.get(address) ?? 1
    return {
      id: address,
      label,
      group: node ? nodeGroup(node) : 'wallet',
      weight: Math.max(1, Math.min(30, rawWeight)),
      borderColor: COMPLETENESS_BORDER[nodeCompleteness(node)],
    }
  })

  const graphEdges = edgeDisplay.aggregate
    ? buildAggregatedEdges(edges, edgeDisplay)
    : buildIndividualEdges(edges, edgeDisplay)

  return { nodes: graphNodes, edges: graphEdges }
}

const UNIFORM_WEIGHT = 4

function buildIndividualEdges(edges: CaseGraphEdge[], display: EdgeDisplaySettings): GraphEdge[] {
  return edges.map((edge, index) => {
    const amount = edgeAmount(edge)
    return {
      id: `${edgeKey(edge)}:${index}`,
      source: edge.from.toLowerCase(),
      target: edge.to.toLowerCase(),
      label: `${edge.formatted} ${edge.symbol}`,
      weight: display.widthByAmount ? weightForAmount(amount) : UNIFORM_WEIGHT,
      color: display.colorByCurrency ? colorForSymbol(edge.symbol) : null,
    }
  })
}

function buildAggregatedEdges(edges: CaseGraphEdge[], display: EdgeDisplaySettings): GraphEdge[] {
  type Bucket = { source: string; target: string; symbol: string; total: number; count: number }
  const buckets = new Map<string, Bucket>()

  for (const edge of edges) {
    const source = edge.from.toLowerCase()
    const target = edge.to.toLowerCase()
    const key = `${source}>${target}:${edge.symbol}`
    const bucket = buckets.get(key)
    if (bucket) {
      bucket.total += edgeAmount(edge)
      bucket.count += 1
    } else {
      buckets.set(key, { source, target, symbol: edge.symbol, total: edgeAmount(edge), count: 1 })
    }
  }

  return Array.from(buckets.entries()).map(([key, bucket]) => {
    const countPrefix = bucket.count > 1 ? `${bucket.count}× ` : ''
    return {
      id: `agg:${key}`,
      source: bucket.source,
      target: bucket.target,
      label: `${countPrefix}${formatAmount(bucket.total)} ${bucket.symbol}`,
      weight: display.widthByAmount ? weightForAmount(bucket.total) : UNIFORM_WEIGHT,
      color: display.colorByCurrency ? colorForSymbol(bucket.symbol) : null,
    }
  })
}
