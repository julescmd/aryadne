//! Expressions des propriétés animées, dans l'esprit d'After Effects : une petite formule calcule la valeur
//! à chaque instant. Nombres et vecteurs [x, y], + − × ÷ % ^, parenthèses, fonctions.
//!
//! Variables : `time` (ou `t`), `value` (la valeur animée sans l'expression), `index` (rang du calque),
//! `fps`, `frame`, `pi`. Fonctions : sin, cos, tan, abs, sqrt, floor, ceil, round, min, max, pow, exp, log,
//! clamp(x, a, b), linear(x, a, b, c, d), ease(x, a, b, c, d), noise(x), random(n), wiggle(fréq, ampl),
//! length(v), mod(a, b), loop(durée) (le temps qui recommence).

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(f64),
    Vec(Vec<f64>),
}

impl Val {
    pub fn num(&self) -> f64 {
        match self {
            Val::Num(x) => *x,
            Val::Vec(v) => v.first().copied().unwrap_or(0.0),
        }
    }

    /// En deux composantes (un nombre devient [n, n])
    pub fn pair(&self) -> [f64; 2] {
        match self {
            Val::Num(x) => [*x, *x],
            Val::Vec(v) => [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)],
        }
    }
}

/// Ce que voit une expression
pub struct Ctx {
    pub time: f64,
    pub value: Val,
    pub index: f64,
    pub fps: f64,
    /// Graine du hasard (propre au calque)
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(char),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() || ch == ';' {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            let txt: String = c[st..i].iter().collect();
            out.push(Tok::Num(txt.parse().map_err(|_| format!("nombre illisible : {txt}"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Id(c[st..i].iter().collect()));
        } else if "+-*/%^(),[]".contains(ch) {
            out.push(Tok::Op(ch));
            i += 1;
        } else {
            return Err(format!("caractère inattendu : {ch}"));
        }
    }
    Ok(out)
}

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    cx: &'a Ctx,
}

fn bin(a: Val, b: Val, f: impl Fn(f64, f64) -> f64) -> Val {
    match (a, b) {
        (Val::Num(x), Val::Num(y)) => Val::Num(f(x, y)),
        (Val::Vec(v), Val::Num(y)) => Val::Vec(v.into_iter().map(|x| f(x, y)).collect()),
        (Val::Num(x), Val::Vec(v)) => Val::Vec(v.into_iter().map(|y| f(x, y)).collect()),
        (Val::Vec(v), Val::Vec(w)) => Val::Vec((0..v.len().max(w.len())).map(|k| f(v.get(k).copied().unwrap_or(0.0), w.get(k).copied().unwrap_or(0.0))).collect()),
    }
}

fn map1(a: Val, f: impl Fn(f64) -> f64) -> Val {
    match a {
        Val::Num(x) => Val::Num(f(x)),
        Val::Vec(v) => Val::Vec(v.into_iter().map(f).collect()),
    }
}

/// Bruit doux entre -1 et 1 (interpolation de valeurs au hasard aux entiers)
fn noise(x: f64, ch: u64, seed: u64) -> f64 {
    let h = |i: i64| {
        let mut v = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ ch.wrapping_mul(0x1656_67B1_9E37_79F9);
        v ^= v >> 31;
        v = v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        v ^= v >> 29;
        (v >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let i = x.floor();
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    h(i as i64) * (1.0 - u) + h(i as i64 + 1) * u
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Op(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("« {c} » attendu"))
        }
    }

    fn sum(&mut self) -> Result<Val, String> {
        let mut a = self.prod()?;
        loop {
            if self.eat('+') {
                a = bin(a, self.prod()?, |x, y| x + y);
            } else if self.eat('-') {
                a = bin(a, self.prod()?, |x, y| x - y);
            } else {
                return Ok(a);
            }
        }
    }

    fn prod(&mut self) -> Result<Val, String> {
        let mut a = self.unary()?;
        loop {
            if self.eat('*') {
                a = bin(a, self.unary()?, |x, y| x * y);
            } else if self.eat('/') {
                a = bin(a, self.unary()?, |x, y| if y == 0.0 { 0.0 } else { x / y });
            } else if self.eat('%') {
                a = bin(a, self.unary()?, |x, y| if y == 0.0 { 0.0 } else { x.rem_euclid(y) });
            } else {
                return Ok(a);
            }
        }
    }

    fn unary(&mut self) -> Result<Val, String> {
        if self.eat('-') {
            return Ok(map1(self.unary()?, |x| -x));
        }
        if self.eat('+') {
            return self.unary();
        }
        let a = self.atom()?;
        if self.eat('^') {
            let b = self.unary()?;
            return Ok(bin(a, b, f64::powf));
        }
        Ok(a)
    }

    fn args(&mut self) -> Result<Vec<Val>, String> {
        let mut v = vec![];
        if self.eat(')') {
            return Ok(v);
        }
        loop {
            v.push(self.sum()?);
            if self.eat(')') {
                return Ok(v);
            }
            self.expect(',')?;
        }
    }

    fn atom(&mut self) -> Result<Val, String> {
        let tok = self.peek().cloned().ok_or("formule incomplète")?;
        self.i += 1;
        let mut v = match tok {
            Tok::Num(x) => Val::Num(x),
            Tok::Op('(') => {
                let v = self.sum()?;
                self.expect(')')?;
                v
            }
            Tok::Op('[') => {
                let mut items = vec![];
                if !self.eat(']') {
                    loop {
                        items.push(self.sum()?.num());
                        if self.eat(']') {
                            break;
                        }
                        self.expect(',')?;
                    }
                }
                Val::Vec(items)
            }
            Tok::Id(name) => {
                if self.eat('(') {
                    let a = self.args()?;
                    self.call(&name, a)?
                } else {
                    self.var(&name)?
                }
            }
            Tok::Op(c) => return Err(format!("« {c} » inattendu")),
        };
        // Composante : value[0]
        while self.eat('[') {
            let k = self.sum()?.num().max(0.0) as usize;
            self.expect(']')?;
            v = Val::Num(match &v {
                Val::Vec(items) => items.get(k).copied().unwrap_or(0.0),
                Val::Num(x) => *x,
            });
        }
        Ok(v)
    }

    fn var(&self, name: &str) -> Result<Val, String> {
        let cx = self.cx;
        Ok(match name {
            "time" | "t" => Val::Num(cx.time),
            "value" => cx.value.clone(),
            "index" => Val::Num(cx.index),
            "fps" => Val::Num(cx.fps),
            "frame" => Val::Num((cx.time * cx.fps).floor()),
            "pi" | "PI" => Val::Num(std::f64::consts::PI),
            _ => return Err(format!("inconnu : {name}")),
        })
    }

    fn call(&self, name: &str, a: Vec<Val>) -> Result<Val, String> {
        let n = |k: usize| a.get(k).map(|v| v.num()).ok_or_else(|| format!("{name} : il manque un argument"));
        let one = |f: fn(f64) -> f64| -> Result<Val, String> { Ok(map1(a.first().cloned().ok_or(format!("{name} : il manque un argument"))?, f)) };
        let cx = self.cx;
        Ok(match name {
            "sin" => one(f64::sin)?,
            "cos" => one(f64::cos)?,
            "tan" => one(f64::tan)?,
            "abs" | "Math.abs" => one(f64::abs)?,
            "sqrt" => one(f64::sqrt)?,
            "floor" | "Math.floor" => one(f64::floor)?,
            "ceil" => one(f64::ceil)?,
            "round" | "Math.round" => one(f64::round)?,
            "exp" => one(f64::exp)?,
            "log" => one(f64::ln)?,
            "min" | "Math.min" => bin(a.first().cloned().unwrap_or(Val::Num(0.0)), a.get(1).cloned().unwrap_or(Val::Num(0.0)), f64::min),
            "max" | "Math.max" => bin(a.first().cloned().unwrap_or(Val::Num(0.0)), a.get(1).cloned().unwrap_or(Val::Num(0.0)), f64::max),
            "pow" => Val::Num(n(0)?.powf(n(1)?)),
            "mod" => Val::Num(n(0)?.rem_euclid(n(1)?.max(1e-9))),
            "clamp" => {
                let (lo, hi) = (n(1)?, n(2)?);
                map1(a[0].clone(), |x| x.clamp(lo.min(hi), hi.max(lo)))
            }
            "linear" | "ease" => {
                // linear(x, a, b, c, d) : x va de a à b, le résultat de c à d (bloqué aux bornes)
                let (x, lo, hi) = (n(0)?, n(1)?, n(2)?);
                let mut u = ((x - lo) / (hi - lo).max(1e-9)).clamp(0.0, 1.0);
                if name == "ease" {
                    u = u * u * (3.0 - 2.0 * u);
                }
                let (c, d) = (a.get(3).cloned().ok_or("il manque la valeur de départ")?, a.get(4).cloned().ok_or("il manque la valeur d'arrivée")?);
                bin(c, d, |c, d| c + (d - c) * u)
            }
            "noise" => Val::Num(noise(n(0)?, 0, cx.seed)),
            "random" => {
                // Une valeur au hasard, fixe pour ce calque : random(max) ou random(min, max)
                let r = (noise(1e3 + n(0).unwrap_or(1.0) * 0.0, 7, cx.seed) + 1.0) / 2.0;
                match a.len() {
                    0 => Val::Num(r),
                    1 => Val::Num(r * n(0)?),
                    _ => Val::Num(n(0)? + r * (n(1)? - n(0)?)),
                }
            }
            "wiggle" => {
                // La valeur, plus des mouvements doux au hasard
                let (f, amp) = (n(0)?, n(1)?);
                let x = cx.time * f;
                match &cx.value {
                    Val::Num(v) => Val::Num(v + noise(x, 0, cx.seed) * amp),
                    Val::Vec(v) => Val::Vec(v.iter().enumerate().map(|(k, c)| c + noise(x, k as u64, cx.seed) * amp).collect()),
                }
            }
            "length" => match a.first() {
                Some(Val::Vec(v)) => Val::Num(v.iter().map(|x| x * x).sum::<f64>().sqrt()),
                Some(Val::Num(x)) => Val::Num(x.abs()),
                None => return Err("length : il manque un argument".into()),
            },
            "loop" => Val::Num(cx.time.rem_euclid(n(0)?.max(1e-6))),
            _ => return Err(format!("fonction inconnue : {name}")),
        })
    }
}

/// Évalue une expression ; une erreur dit ce qui ne va pas, en français
pub fn eval(src: &str, cx: &Ctx) -> Result<Val, String> {
    let t = lex(src)?;
    if t.is_empty() {
        return Ok(cx.value.clone());
    }
    let mut p = P { t, i: 0, cx };
    let v = p.sum()?;
    if p.i < p.t.len() {
        return Err("la formule continue après la fin".into());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx(time: f64, value: Val) -> Ctx {
        Ctx { time, value, index: 2.0, fps: 24.0, seed: 7 }
    }

    #[test]
    fn expressions() {
        let c = cx(2.0, Val::Vec(vec![100.0, 50.0]));
        assert_eq!(eval("time * 90", &c).unwrap(), Val::Num(180.0));
        assert_eq!(eval("value + [10, -10]", &c).unwrap(), Val::Vec(vec![110.0, 40.0]));
        assert_eq!(eval("value[1] * 2", &c).unwrap(), Val::Num(100.0));
        assert_eq!(eval("2 ^ 3 + -1", &c).unwrap(), Val::Num(7.0));
        assert_eq!(eval("linear(time, 0, 4, 0, 100)", &c).unwrap(), Val::Num(50.0));
        assert_eq!(eval("clamp(time * 100, 0, 150)", &c).unwrap(), Val::Num(150.0));
        assert_eq!(eval("frame", &c).unwrap(), Val::Num(48.0));
        assert_eq!(eval("loop(1.5)", &c).unwrap(), Val::Num(0.5));
        assert_eq!(eval("", &c).unwrap(), c.value);
        // wiggle reste près de la valeur, et change avec le temps
        let w = eval("wiggle(2, 10)", &c).unwrap().pair();
        assert!((w[0] - 100.0).abs() <= 10.0 && (w[1] - 50.0).abs() <= 10.0);
        assert_ne!(eval("wiggle(2, 10)", &cx(2.3, c.value.clone())).unwrap().pair(), w);
        // Erreurs lisibles
        assert!(eval("time +", &c).unwrap_err().contains("incomplète"));
        assert!(eval("truc(1)", &c).unwrap_err().contains("inconnue"));
        assert!(eval("sin(time", &c).is_err());
    }
}
