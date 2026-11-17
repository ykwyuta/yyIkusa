use crate::general_radio::EXPRESSIONS;
use crate::generals::GENERALS;
use crate::lang::{self, Lang};

pub fn line(general: usize, cue: usize, variant: usize) -> &'static str {
    match lang::lang() {
        Lang::Ja => JAPANESE[general][cue][variant],
        Lang::En => ENGLISH[cue][variant],
        Lang::Zh => CHINESE[cue][variant],
    }
}

const ENGLISH: [[&str; 2]; EXPRESSIONS] = [
    [
        "Squad in position. We are ready.",
        "All troops, assemble. We are about to begin.",
    ],
    [
        "Troops, advance! Keep your spacing.",
        "All troops, move out. Follow our banner.",
    ],
    [
        "Hold this position until the next order.",
        "Halt and watch your sector. Report anything unusual.",
    ],
    [
        "Enemy troops spotted. Engage when they are in reach.",
        "We have contact. Report enemy positions.",
    ],
    [
        "We are taking concentrated fire. Watch your flanks.",
        "Enemy pressure is building. Wounded troops, fall back.",
    ],
    [
        "Commander-in-chief, we need reinforcements at our position!",
        "Requesting support! Cover our damaged troops while they fall back.",
    ],
    [
        "We lost a unit. Remaining troops, regroup.",
        "We have taken losses. All surviving troops, report in.",
    ],
    [
        "Enemy fire has eased. Reorganize the ranks.",
        "Contact broken. We can regroup now.",
    ],
    [
        "We are inside the enemy base. Hold until the capture is complete.",
        "Objective secured for now. Keep watch while we capture it.",
    ],
    [
        "Commander-in-chief, victory confirmed. Well done, everyone.",
        "Mission accomplished. All surviving troops, prepare to return.",
    ],
];

const CHINESE: [[&str; 2]; EXPRESSIONS] = [
    ["部队已就位，可以开始。", "各队请报到，行动即将开始。"],
    ["部队前进！保持间距。", "全军出发，跟随军旗。"],
    [
        "在这里坚守，等待下一条命令。",
        "停止前进，保持警戒。发现异常立即报告。",
    ],
    [
        "发现敌军。进入攻击范围后迎击。",
        "与敌军接触。各队报告目标位置。",
    ],
    ["敌方火力集中，注意侧翼。", "敌军攻势增强。受损部队请报告。"],
    [
        "总司令，请向我方位置派遣援军！",
        "请求支援！请掩护受损部队后撤。",
    ],
    [
        "损失一队部队。其余部队重新集结。",
        "我方出现损失。幸存将士请报到。",
    ],
    [
        "敌方火力减弱。各队检查状态。",
        "已脱离接触，现在可以重新集结。",
    ],
    [
        "已进入敌军基地。在占领完成前坚守。",
        "暂时控制目标。占领期间继续警戒。",
    ],
    ["总司令，确认获胜。各位辛苦了。", "任务完成。各队准备返回。"],
];

const COMMON: [[&str; 2]; EXPRESSIONS] = [
    ["布陣を終えた。進軍の合図を待つ。","皆の者、旗のもとへ集まれ。"],
    ["全軍、前へ！","隊列を保って進め！"],
    ["ここを守れ。","次の合図まで待機せよ。"],
    ["敵を見つけた。迎え撃て！","敵が来る。狙いを定めよ。"],
    ["敵の攻勢が激しい。側面を守れ！","隊列を立て直せ。"],
    ["総大将、援軍を願う！","味方の後退を援護してくれ！"],
    ["味方に損害。残る兵を集めよ。","旗を見失うな。再集結せよ。"],
    ["敵の攻勢が弱まった。態勢を整えよ。","いったん集結する。"],
    ["敵の本陣に入った。守りを固めよ！","占領を終えるまで持ちこたえよ。"],
    ["勝利だ。皆、よく戦った。","戦は終わった。兵を休ませよ。"],
];
const JAPANESE: [[[&str; 2]; EXPRESSIONS]; GENERALS.len()] = [COMMON; GENERALS.len()];
