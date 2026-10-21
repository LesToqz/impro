//! Character-indexed fuzzy matching, stable ranking and markup helpers.
use std::sync::atomic::{AtomicBool, Ordering};
pub type Range = (usize, usize);
pub type Match = (f64, Vec<Range>);
pub const MAX_RESULTS: usize = 100;
pub const BOUNDARY: f64 = 8.;
pub const CONSECUTIVE: f64 = 4.;
pub const FRAGMENT: f64 = 4.;
pub const GAP: f64 = 0.5;
pub const LATE: f64 = 0.2;
pub const LENGTH: f64 = 0.03;
pub const NAME: f64 = 10.;
pub const PREFIX: f64 = 3.;
pub const TOKEN: f64 = 2.;
pub const EXACT_NAME: f64 = 15.;
pub const CASE: f64 = 0.1;
pub const CAP: usize = 40;
/// Lowercase each character only when it remains one character.
pub fn fold(text: &str) -> String {
    text.chars().map(|c| { let mut lower=c.to_lowercase(); let first=lower.next().unwrap_or(c); if lower.next().is_none() {first} else {c} }).collect()
}
pub fn name_start(text: &str) -> usize {
    text.chars().enumerate().filter(|(_,c)| matches!(c,'/'|'\\')).map(|(i,_)|i+1).last().unwrap_or(0)
}
fn boundary(text: &[char], i: usize) -> bool {
    i==0 || !text[i-1].is_alphanumeric() || !text[i].is_alphanumeric() || (text[i].is_uppercase() && text[i-1].is_lowercase()) || text[i].is_numeric()!=text[i-1].is_numeric()
}
fn find(text: &[char], token: &[char], start: usize) -> Option<usize> {
    if token.is_empty() { return Some(start); }
    (start..=text.len().checked_sub(token.len())?).find(|&i| text[i..i+token.len()]==*token)
}
fn tokens(text: &[char], low: &[char], ts: &[Vec<char>], start: usize, prefer: bool) -> Option<Vec<Range>> {
    let mut ranges=Vec::with_capacity(ts.len()); let mut pos=start;
    for t in ts { let first=find(low,t,pos)?; let mut index=first;
        if prefer { let mut candidate=Some(first); while let Some(i)=candidate { if boundary(text,i) {index=i; break;} candidate=find(low,t,i+1); } }
        ranges.push((index,index+t.len())); pos=index+t.len();
    } Some(ranges)
}
fn merge(indices: &[usize]) -> Vec<Range> {
    let mut ranges: Vec<Range>=Vec::new(); for &i in indices {if let Some(last)=ranges.last_mut() {if last.1==i {last.1=i+1;continue;}} ranges.push((i,i+1));} ranges
}
fn compact(low: &[char], chars: &[char], start: usize) -> Option<Vec<usize>> {
    let mut pos=start; for c in chars {pos+=low.get(pos..)?.iter().position(|v|v==c)?+1;}
    let mut result=Vec::with_capacity(chars.len()); for c in chars.iter().rev() {pos=start+low[start..pos].iter().rposition(|v|v==c)?; result.push(pos);}
    result.reverse(); Some(result)
}
fn aligned(text: &[char], low: &[char], chars: &[char], start: usize) -> Option<Vec<usize>> {
    let mut result=Vec::with_capacity(chars.len()); let mut pos=start;
    for c in chars { let first=pos+low.get(pos..)?.iter().position(|v|v==c)?; let mut i=first;
        if result.is_empty() || first!=pos { if let Some(next)=(first..low.len()).find(|&j|low[j]==*c && boundary(text,j)) {i=next;} }
        result.push(i);pos=i+1;
    } Some(result)
}
fn score(text: &[char], ranges: &[Range], query: &[char], start: usize, base: f64) -> f64 {
    let (first,last)=(ranges[0].0,ranges[ranges.len()-1].1); let mut value=base; let mut matched=0;
    for &(a,b) in ranges {if a==start || boundary(text,a) {value+=BOUNDARY;} value+=CONSECUTIVE*(b-a-1) as f64;matched+=b-a;}
    value-=FRAGMENT*(ranges.len()-1) as f64;value-=GAP*(last-first-matched).min(CAP) as f64;
    value-=LATE*(first-start).min(CAP) as f64;value-=LENGTH*text.len().min(4*CAP) as f64;
    if first==start {value+=PREFIX;}
    if query.iter().any(|c|c.is_uppercase()) {let mut position=0;for &(a,b) in ranges {for &c in &text[a..b] {if query.get(position)==Some(&c) {value+=CASE;} position+=1;}}} value
}
struct Prepared { tokens: Vec<Vec<char>>, chars: Vec<char>, original: Vec<char> }
impl Prepared {fn new(query: &str)->Self {let tokens:Vec<Vec<char>>=fold(query).split_whitespace().map(|s|s.chars().collect()).collect();let chars=tokens.iter().flatten().copied().collect();Self{tokens,chars,original:query.split_whitespace().collect::<String>().chars().collect()}}}
#[derive(Clone)]
struct Entry {text: Vec<char>, low: Vec<char>, start: usize}
fn matched(entry: &Entry, q: &Prepared, path: bool)->Option<Match> {
    if q.tokens.is_empty() {return Some((0.,vec![]));}
    let (text,low)=(&entry.text,&entry.low);let mut pos=0;
    for c in &q.chars {pos+=low.get(pos..)?.iter().position(|v|v==c)?+1;}
    let starts=if path && entry.start>0 {vec![entry.start,0]} else {vec![0]};
    let mut best: Option<Match>=None;
    let mut consider=|ranges: Vec<Range>,start,base| {let bonus=if path && ranges[0].0>=entry.start {NAME} else {0.};let value=score(text,&ranges,&q.original,start,base+bonus);if best.as_ref().is_none_or(|(old,_)|value>*old) {best=Some((value,ranges));}};
    let mut found=false;
    for &s in &starts {if let Some(ranges)=tokens(text,low,&q.tokens,s,true).or_else(||tokens(text,low,&q.tokens,s,false)) {consider(ranges,s,TOKEN);found=true;}}
    if !found {for &s in &starts {let mut any=false;for indices in [compact(low,&q.chars,s),aligned(text,low,&q.chars,s)].into_iter().flatten() {consider(merge(&indices),s,0.);any=true;} if any && path {break;}}}
    if let Some((value,_))=&mut best {if path {let name=&low[entry.start..];let stem=if let Some(dot)=name.iter().rposition(|c|*c=='.').filter(|i|*i>0) {&name[..dot]} else {name};if q.chars==name || q.chars==stem {*value+=EXACT_NAME;}}} best
}
/// Return a score and half-open character ranges, or no match.
pub fn r#match(query: &str,text: &str,path: bool)->Option<Match> {
    matched(&Entry{text:text.chars().collect(),low:fold(text).chars().collect(),start:if path {name_start(text)} else {0}},&Prepared::new(query),path)
}
/// Precomputed text with incremental narrowing; candidates retain source order.
pub struct FuzzyIndex<T> {pub candidates:Vec<T>, entries:Vec<Entry>, path:bool, last_query:String,last_hits:Vec<usize>}
impl<T: Clone> FuzzyIndex<T> {
    pub fn new(candidates:Vec<T>,key:impl Fn(&T)->String,path:bool)->Self {
        let entries=candidates.iter().map(|v|{let text=key(v);Entry{start:if path {name_start(&text)} else {0},low:fold(&text).chars().collect(),text:text.chars().collect()}}).collect();
        Self{candidates,entries,path,last_query:String::new(),last_hits:vec![]}
    }
    pub fn len(&self)->usize {self.candidates.len()}
    pub fn is_empty(&self)->bool {self.candidates.is_empty()}
    pub fn search(&mut self,query:&str,limit:Option<usize>)->Vec<(T,f64,Vec<Range>)> {self.search_cancelled(query,limit,None)}
    /// Cancel checks occur every 64 candidates; cancelled searches return no rows.
    pub fn search_cancelled(&mut self,query:&str,limit:Option<usize>,cancel:Option<&AtomicBool>)->Vec<(T,f64,Vec<Range>)> {
        let q=Prepared::new(query);if q.tokens.is_empty() {self.last_query.clear();self.last_hits.clear();return self.candidates.iter().take(limit.unwrap_or(usize::MAX)).cloned().map(|v|(v,0.,vec![])).collect();}
        let pool:Vec<usize>=if !self.last_query.trim().is_empty() && query.starts_with(&self.last_query) {std::mem::take(&mut self.last_hits)} else {(0..self.len()).collect()};
        let mut scored=Vec::new();let mut hits=Vec::new();
        for (n,i) in pool.into_iter().enumerate() {if n%64==0 && cancel.is_some_and(|c|c.load(Ordering::Relaxed)) {self.last_query.clear();return vec![];} if let Some((score,ranges))=matched(&self.entries[i],&q,self.path) {hits.push(i);scored.push((i,score,ranges));}}
        self.last_query=query.into();self.last_hits=hits;
        let compare=|a:&(usize,f64,Vec<Range>),b:&(usize,f64,Vec<Range>)|b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
        if let Some(n)=limit {if n<scored.len() {scored.select_nth_unstable_by(n,compare);scored.truncate(n);}}
        scored.sort_unstable_by(compare);scored.into_iter().map(|(i,s,r)|(self.candidates[i].clone(),s,r)).collect()
    }
}
pub fn rank(query:&str,candidates:&[String],path:bool,limit:Option<usize>)->Vec<(String,f64,Vec<Range>)> {FuzzyIndex::new(candidates.to_vec(),Clone::clone,path).search(query,limit)}
pub fn highlight_markup(text:&str,ranges:&[Range])->String {
    let chars:Vec<char>=text.chars().collect();let mut ranges=ranges.to_vec();ranges.sort_unstable();let mut out=String::new();let mut pos=0;
    fn escape(out:&mut String,chars:&[char]) {for c in chars {match c {'&'=>out.push_str("&amp;"),'<'=>out.push_str("&lt;"),'>'=>out.push_str("&gt;"),c=>out.push(*c)}}}
    for (a,b) in ranges {let a=a.max(pos);let b=b.min(chars.len());if a>=b {continue;}escape(&mut out,&chars[pos..a]);out.push_str("<b>");escape(&mut out,&chars[a..b]);out.push_str("</b>");pos=b;}escape(&mut out,&chars[pos..]);out
}
pub fn split_path_ranges(path:&str,ranges:&[Range])->(Vec<Range>,Vec<Range>) {
    let start=name_start(path);let end=start.saturating_sub(1);let mut folder=vec![];let mut name=vec![];
    for &(a,b) in ranges {if a<end {folder.push((a,b.min(end)));}if b>start {name.push((a.max(start)-start,b-start));}}(folder,name)
}
pub fn prompt_geometry(width:i32,height:i32)->(i32,i32,i32,i32) {let w=700.min((width as f64*0.8) as i32);((width-w)/2,(height as f64*0.15).round_ties_even() as i32,w,(height as f64*0.7) as i32)}
