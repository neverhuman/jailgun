export interface MiniChartDatum {
  label: string;
  value: number;
}

export interface MiniChartProps {
  data: MiniChartDatum[];
  type: 'line' | 'bar';
  color: string;
  height?: number;
}

const PADDING_X = 8;
const PADDING_TOP = 8;
const PADDING_BOTTOM = 4;

export function MiniChart({ data, type, color, height = 80 }: MiniChartProps) {
  if (data.length === 0) {
    return <p className="muted miniChartEmpty">No data</p>;
  }

  const viewBoxWidth = 300;
  const viewBoxHeight = height;
  const chartWidth = viewBoxWidth - PADDING_X * 2;
  const chartHeight = viewBoxHeight - PADDING_TOP - PADDING_BOTTOM;
  const maxValue = Math.max(...data.map((d) => d.value), 1);

  if (type === 'line') {
    return (
      <svg
        className="miniChart"
        viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`}
        preserveAspectRatio="none"
        role="img"
        aria-label="line chart"
      >
        <LineChart
          data={data}
          color={color}
          maxValue={maxValue}
          chartWidth={chartWidth}
          chartHeight={chartHeight}
          paddingX={PADDING_X}
          paddingTop={PADDING_TOP}
        />
      </svg>
    );
  }

  return (
    <svg
      className="miniChart"
      viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="bar chart"
    >
      <BarChart
        data={data}
        color={color}
        maxValue={maxValue}
        chartWidth={chartWidth}
        chartHeight={chartHeight}
        paddingX={PADDING_X}
        paddingTop={PADDING_TOP}
      />
    </svg>
  );
}

interface ChartInnerProps {
  data: MiniChartDatum[];
  color: string;
  maxValue: number;
  chartWidth: number;
  chartHeight: number;
  paddingX: number;
  paddingTop: number;
}

function LineChart({ data, color, maxValue, chartWidth, chartHeight, paddingX, paddingTop }: ChartInnerProps) {
  const points = data.map((d, i) => {
    const x = paddingX + (data.length === 1 ? chartWidth / 2 : (i / (data.length - 1)) * chartWidth);
    const y = paddingTop + chartHeight - (d.value / maxValue) * chartHeight;
    return { x, y, label: d.label, value: d.value };
  });

  const pathD = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.y}`).join(' ');
  const first = points[0];
  const last = points[points.length - 1];
  const areaD = `${pathD} L ${last.x} ${paddingTop + chartHeight} L ${first.x} ${paddingTop + chartHeight} Z`;

  return (
    <>
      <path d={areaD} fill={color} opacity={0.12} className="miniChartArea" />
      <path d={pathD} fill="none" stroke={color} strokeWidth={2} className="miniChartLine" />
      {points.map((p, i) => (
        <circle key={i} cx={p.x} cy={p.y} r={3} fill={color}>
          <title>{`${p.label}: ${p.value}`}</title>
        </circle>
      ))}
    </>
  );
}

function BarChart({ data, color, maxValue, chartWidth, chartHeight, paddingX, paddingTop }: ChartInnerProps) {
  const barGap = 4;
  const barWidth = Math.max(2, (chartWidth - barGap * (data.length - 1)) / data.length);

  return (
    <>
      {data.map((d, i) => {
        const barHeight = Math.max(2, (d.value / maxValue) * chartHeight);
        const x = paddingX + i * (barWidth + barGap);
        const y = paddingTop + chartHeight - barHeight;
        return (
          <rect
            key={i}
            x={x}
            y={y}
            width={barWidth}
            height={barHeight}
            rx={2}
            ry={2}
            fill={color}
            className="miniChartBar"
          >
            <title>{`${d.label}: ${d.value}`}</title>
          </rect>
        );
      })}
    </>
  );
}
