# Quota Critter 视觉与交互规范

## 1. 方向

Quota Critter 使用原创像素星灵 **Lumo** 表达额度状态。必须与“米白机械机器人 + 橙黄色电量条”保持明显差异。

关键词：celestial、calm、precise、tiny、original。

## 2. Lumo

- 不对称四角星/软菱形身体；
- 轻微不对称、带倾斜尖角的软菱形身体；
- 一个独立漂浮卫星点；
- 深靛蓝眼睛和薄荷色面颊；
- 不使用手臂、机械关节、金属面板或天线。

Sprite 建议：48×48 px 网格，CSS 显示 72–96 px，透明 PNG/WebP，使用 `image-rendering: pixelated`；每个循环状态最多 4 帧，Reset 最多 6 帧。

## 3. 色彩 Token

| Token | 色值 | 用途 |
| --- | --- | --- |
| `--bg-canvas` | `#17162B` | 画布 |
| `--bg-panel` | `#242344` | 面板 |
| `--bg-panel-raised` | `#2D2B52` | 悬停/选中 |
| `--border-subtle` | `#484775` | 边框 |
| `--lumo-primary` | `#8D96FF` | 角色主体 |
| `--quota-good` | `#74E0C1` | 额度/在线 |
| `--quota-low` | `#FF7D7D` | 低额度/错误 |
| `--text-primary` | `#F3F1FF` | 主文字 |
| `--text-secondary` | `#AAA8D2` | 次要文字 |
| `--text-disabled` | `#716F98` | 失效/旧数据 |

首版禁用大面积橙色、琥珀色、米白、暖象牙白、金属银灰、黄色电池条和赛博霓虹渐变。

## 4. 尺寸

### 默认挂件

- 300×152 logical px 的透明舞台，不绘制大方块背景；
- Lumo 为主要视觉焦点，点击命中区约 150×142 px；
- 斜向半弧轨道位于 Lumo 右侧，轨道节点表达额度状态；
- 不使用大面积模糊。

### 默认收起态

- 只展示 Lumo 与右侧斜向半弧轨道；
- 不展示百分比、进度条或重置时间；
- 点击 Lumo 展开详情，视觉重心始终在星灵。

### 详细面板

- 展开后在同一舞台下方显示额度信息；
- 展示百分比、使用量/上限、进度条与重置时间；
- 刷新动作放在系统托盘菜单，不占用主视觉。

## 5. 额度轨道

- 固定 10 个节点；
- 点亮节点数为 `round(remainingPercent / 10)`；
- 精确值永远以数字为准；
- 低于 20% 时末端节点改为珊瑚色；
- 只在新数据或重置时播放一次补间动画；
- 不用无限旋转加载环冒充额度。

## 6. 状态与动画

| 状态 | 视觉 | 动画 |
| --- | --- | --- |
| Full | 薄荷、上浮 | 卫星点 1800ms 闪烁 |
| Steady | 长春花、平静 | 2400ms 呼吸，位移 ≤2px |
| Low | 珊瑚提示、下沉 | 1600ms 脉冲，不抖动 |
| Exhausted | 低饱和、闭眼 | 静态 |
| Reset | 轨道环绕 | 1200ms、只播放一次 |
| Stale | 降低饱和度 | 无循环 |
| Unknown | 轮廓占位 | 无循环 |

启用系统 reduced motion 时，关闭所有循环动画；Reset 使用静态星环帧。

## 7. 字体与文案

UI 使用系统字体栈：`-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`。数字使用 tabular numerals。主百分比 30px/700，正文 12–14px。

英文：`64% left`、`Resets in 2h 14m`、`Last synced 1m ago`、`Offline — showing last known quota`、`Sign in with ChatGPT`。

中文：`剩余 64%`、`2 小时 14 分后重置`、`1 分钟前同步`、`当前离线，显示上次额度`。

不得使用“无限”“绝对实时”等无法保证的表述。

## 8. 交互

- 单击 Lumo：展开/关闭详情；
- 展开后再次单击 Lumo：收起详情；
- 拖动空白区域：移动挂件；
- 右键：原生菜单；
- `Esc`：关闭详情；
- 锁定后禁用拖动；
- 近屏幕边缘 12px 吸附；
- 不抢占当前应用焦点，除非用户主动打开设置。

## 9. 原创边界

- 不复刻参考图的机器人轮廓、耳罩、米白外壳或橙色槽；
- 不仿制已有游戏、系统或开发工具吉祥物；
- 不把 OpenAI/VS Code 标志作为角色的一部分；
- README 标注“Unofficial open-source companion; not affiliated with OpenAI”。

## 10. 视觉验收

缩放 100% 时数字清晰；角色隐藏后额度信息完整；黑白截图仍可区分 Low；100%–200% DPI 无错位；reduced motion 下无循环动画。
