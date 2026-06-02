import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';

import { MiniChart } from './MiniChart';

it('renders SVG with correct number of bars for bar type', () => {
  const data = [
    { label: 'A', value: 10 },
    { label: 'B', value: 20 },
    { label: 'C', value: 30 }
  ];
  render(<MiniChart data={data} type="bar" color="#4a7c59" />);
  const svg = screen.getByRole('img', { name: 'bar chart' });
  expect(svg).toBeInTheDocument();
  const bars = svg.querySelectorAll('rect');
  expect(bars).toHaveLength(3);
});

it('renders SVG path for line type', () => {
  const data = [
    { label: 'A', value: 10 },
    { label: 'B', value: 20 }
  ];
  render(<MiniChart data={data} type="line" color="#2f7d42" />);
  const svg = screen.getByRole('img', { name: 'line chart' });
  expect(svg).toBeInTheDocument();
  const paths = svg.querySelectorAll('path');
  expect(paths.length).toBeGreaterThanOrEqual(1);
});

it('renders No data message for empty data', () => {
  render(<MiniChart data={[]} type="bar" color="#4a7c59" />);
  expect(screen.getByText('No data')).toBeInTheDocument();
});

it('applies color prop to bars', () => {
  const data = [{ label: 'A', value: 10 }];
  render(<MiniChart data={data} type="bar" color="#b43b33" />);
  const svg = screen.getByRole('img', { name: 'bar chart' });
  const rect = svg.querySelector('rect');
  expect(rect).toHaveAttribute('fill', '#b43b33');
});

it('applies color prop to line stroke', () => {
  const data = [
    { label: 'A', value: 10 },
    { label: 'B', value: 20 }
  ];
  render(<MiniChart data={data} type="line" color="#5a8bbf" />);
  const svg = screen.getByRole('img', { name: 'line chart' });
  const line = svg.querySelector('.miniChartLine');
  expect(line).toHaveAttribute('stroke', '#5a8bbf');
});
