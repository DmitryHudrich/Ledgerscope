import { formatDuration, type BuildJob, type BuildStage } from '../../api/monitor';
import { formatCount } from '../../lib/format';
import { cx } from '../ui';

const STAGES: { stage: BuildStage; label: string; unit: string }[] = [
  { stage: 'coverage', label: 'Coverage', unit: 'ranges' },
  { stage: 'walk', label: 'Walking', unit: 'addresses' },
  { stage: 'index', label: 'Reading index', unit: 'ranges' },
  { stage: 'classify', label: 'Classifying', unit: 'txs' },
  { stage: 'actors', label: 'Resolving', unit: 'addresses' },
  { stage: 'tags', label: 'Tagging', unit: '' },
];

export function BuildProgress({ job, sweep }: { job: BuildJob; sweep: boolean }) {
  const percent = Math.min(100, Math.max(0, job.percent));
  const shown = STAGES.filter(({ stage }) => (sweep ? stage !== 'walk' : stage !== 'index' && stage !== 'classify'));
  const current = shown.findIndex(({ stage }) => stage === job.stage);
  const info = STAGES.find(({ stage }) => stage === job.stage);

  return (
    <div className="grid w-full min-w-0 gap-2 text-left">
      <div className="flex items-baseline justify-between gap-3 text-xs">
        <span className="font-medium text-text-primary">
          {info?.label ?? job.stage}
          {info?.unit && job.total > 0 && (
            <span className="tabular-nums text-text-secondary"> · {formatCount(job.done)} / {formatCount(job.total)} {info.unit}</span>
          )}
        </span>
        <span className="text-[15px] font-semibold tabular-nums">{percent.toFixed(0)}%</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-hairline" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
        <div className="h-full rounded-full bg-accent [transition:width_300ms_ease]" style={{ width: `${percent}%` }} />
      </div>
      <ol className="m-0 flex list-none flex-wrap gap-x-3 gap-y-0.5 p-0 text-[11px]">
        {shown.map(({ stage, label }, index) => (
          <li key={stage} className={cx(index < current ? 'text-text-secondary' : index === current ? 'font-medium text-accent' : 'text-text-muted')}>
            {index < current ? '✓ ' : ''}{label}
          </li>
        ))}
      </ol>
      <div className="flex justify-between text-[11px] tabular-nums text-text-secondary">
        <span>{formatCount(job.txs)} txs read</span>
        <span>elapsed {formatDuration(Date.now() - job.started_ms)}</span>
      </div>
    </div>
  );
}
