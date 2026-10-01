// Registers only the ECharts pieces this app uses. Loaded via dynamic import from
// EChart.tsx so screens without charts never download echarts.
import { BarChart, LineChart, PieChart, RadarChart, SankeyChart, ScatterChart } from 'echarts/charts';
import {
  DataZoomComponent,
  DatasetComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  MarkPointComponent,
  RadarComponent,
  TitleComponent,
  TooltipComponent,
  TransformComponent,
  VisualMapComponent,
} from 'echarts/components';
import * as echarts from 'echarts/core';
import { LabelLayout, UniversalTransition } from 'echarts/features';
import { CanvasRenderer, SVGRenderer } from 'echarts/renderers';

echarts.use([
  BarChart,
  LineChart,
  PieChart,
  RadarChart,
  SankeyChart,
  ScatterChart,
  DataZoomComponent,
  DatasetComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  MarkPointComponent,
  RadarComponent,
  TitleComponent,
  TooltipComponent,
  TransformComponent,
  VisualMapComponent,
  LabelLayout,
  UniversalTransition,
  CanvasRenderer,
  SVGRenderer,
]);

export { echarts };
