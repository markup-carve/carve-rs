use carve::html_import::{html_to_carve, html_to_ast, HtmlImportOptions, HtmlImportAdapter};
use std::{fs,env,time::Instant};
fn main() {
 let args:Vec<String>=env::args().collect();
 let shape=&args[1];
 let options=HtmlImportOptions{adapter:HtmlImportAdapter::Word,..HtmlImportOptions::default()};
 if shape=="semantic" {
  let fixtures:serde_json::Value=serde_json::from_str(&fs::read_to_string("/tmp/carve-footnote-semantic-fixtures.json").unwrap()).unwrap();
  for item in fixtures.as_array().unwrap() {
   match html_to_carve(item["html"].as_str().unwrap(),&options) {
    Ok(result) => println!("{}",serde_json::json!({"name":item["name"],"value":result.value,"report":format!("{:?}",result.report)})),
    Err(error) => println!("{}",serde_json::json!({"name":item["name"],"error":format!("{:?}",error)})),
   }
  }
  return;
 }
 let api=args.get(4).map(String::as_str).unwrap_or("carve");
 for n in args.get(2).map(String::as_str).unwrap_or("256,1024,4096").split(',') {
  let source=fs::read_to_string(format!("/tmp/carve-footnote-fixtures/{shape}-{n}.html")).unwrap();
  let call=|| if api=="ast" {html_to_ast(&source,&options).map(|result|format!("{:?}\n{:?}",result.value,result.report)).unwrap_or_else(|error|format!("error:{:?}",error))}
    else {html_to_carve(&source,&options).map(|result|format!("{}\n{:?}",result.value,result.report)).unwrap_or_else(|error|format!("error:{:?}",error))};
  call();
  let mut samples=Vec::new();let mut files=Vec::new();
  for i in 0..3 {
   let start=Instant::now();
   // Keep report formatting and file I/O outside the measured API call.
   let (elapsed,output)=if api=="ast" {
    let result=html_to_ast(&source,&options);let elapsed=start.elapsed();
    (elapsed,result.map(|result|format!("{:?}\n{:?}",result.value,result.report)).unwrap_or_else(|error|format!("error:{:?}",error)))
   } else {
    let result=html_to_carve(&source,&options);let elapsed=start.elapsed();
    (elapsed,result.map(|result|format!("{}\n{:?}",result.value,result.report)).unwrap_or_else(|error|format!("error:{:?}",error)))
   };
   samples.push(elapsed.as_secs_f64()*1000.0);
   let file=format!("/tmp/carve-html-perf-{}-{shape}-{n}-{i}.output",args.get(3).map(String::as_str).unwrap_or("worker"));
   fs::write(&file,output).unwrap();files.push(file);
  }
  println!("{}",serde_json::json!({"n":n.parse::<usize>().unwrap(),"bytes":source.len(),"api":api,"samples":samples,"files":files}));
 }
}
