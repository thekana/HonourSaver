use std::collections::HashSet;
use std::io::Cursor;

use larian_formats::lsf::LsfData;
use xmltree::{Element, XMLNode};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const DISABLED: &str = "DisabledSingleSaveSessions";

/// Read every GUID under the DisabledSingleSaveSessions node (de-duplicated, in order).
pub fn list_guids(lsf: &[u8]) -> Result<Vec<String>> {
    let dom = lsf_to_dom(lsf)?;
    Ok(collect_guids(&dom))
}

/// Remove the save whose GUID attribute equals `guid`, returning new LSF bytes.
pub fn reactivate(lsf: &[u8], guid: &str) -> Result<Vec<u8>> {
    let mut dom = lsf_to_dom(lsf)?;
    if !remove_guid(&mut dom, guid) {
        return Err(format!("save not found: {guid}").into());
    }
    let xml = dom_to_xml(&dom)?;
    let data = LsfData::read_lsx(&mut Cursor::new(xml))?;
    let mut out = Vec::new();
    data.write_lsf(&mut out)?;
    Ok(out)
}

fn lsf_to_dom(lsf: &[u8]) -> Result<Element> {
    let data = LsfData::read_lsf(&mut Cursor::new(lsf))?;
    let mut xml = Vec::new();
    data.write_lsx(&mut xml)?;
    Ok(Element::parse(Cursor::new(xml))?)
}

fn dom_to_xml(dom: &Element) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    dom.write(&mut out)?;
    Ok(out)
}

fn is_disabled(elem: &Element) -> bool {
    elem.name == "node" && elem.attributes.get("id").map(String::as_str) == Some(DISABLED)
}

fn find_disabled(elem: &Element) -> Option<&Element> {
    if is_disabled(elem) {
        return Some(elem);
    }
    elem.children.iter().find_map(|c| match c {
        XMLNode::Element(e) => find_disabled(e),
        _ => None,
    })
}

fn find_disabled_mut(elem: &mut Element) -> Option<&mut Element> {
    if is_disabled(elem) {
        return Some(elem);
    }
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            if let Some(found) = find_disabled_mut(e) {
                return Some(found);
            }
        }
    }
    None
}

fn collect_guids(dom: &Element) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(disabled) = find_disabled(dom) {
        collect_guid_attrs(disabled, &mut out);
    }
    let mut seen = HashSet::new();
    out.retain(|g| seen.insert(g.clone()));
    out
}

fn collect_guid_attrs(elem: &Element, out: &mut Vec<String>) {
    for child in &elem.children {
        if let XMLNode::Element(e) = child {
            if is_guid_attribute(e) {
                if let Some(v) = e.attributes.get("value") {
                    out.push(v.clone());
                }
            }
            collect_guid_attrs(e, out);
        }
    }
}

fn is_guid_attribute(elem: &Element) -> bool {
    elem.name == "attribute" && elem.attributes.get("type").map(String::as_str) == Some("guid")
}

// Matches only if the GUID is a *direct* attribute child of `elem`. Real
// profile8.lsf stores each save's guid directly on its node (flat layout);
// unlike the C# original's recursive `Descendants` match, this avoids deleting
// an enclosing container node.
fn has_direct_guid(elem: &Element, guid: &str) -> bool {
    elem.children.iter().any(|c| match c {
        XMLNode::Element(e) => {
            is_guid_attribute(e)
                && e.attributes.get("value").map(String::as_str) == Some(guid)
        }
        _ => false,
    })
}

fn remove_guid(dom: &mut Element, guid: &str) -> bool {
    match find_disabled_mut(dom) {
        Some(disabled) => remove_matching_nodes(disabled, guid),
        None => false,
    }
}

/// Depth-first: drop every descendant `<node>` (never the `disabled` element itself)
/// that directly owns an `<attribute type="guid" value=guid>`.
fn remove_matching_nodes(elem: &mut Element, guid: &str) -> bool {
    let mut removed = false;
    let mut kept = Vec::with_capacity(elem.children.len());
    for mut child in std::mem::take(&mut elem.children) {
        if let XMLNode::Element(ref mut e) = child {
            if e.name == "node" && has_direct_guid(e, guid) {
                removed = true;
                continue;
            }
            if remove_matching_nodes(e, guid) {
                removed = true;
            }
        }
        kept.push(child);
    }
    elem.children = kept;
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<save>
  <version major="4" minor="0" revision="0" build="0"/>
  <region id="Config">
    <node id="Config">
      <children>
        <node id="DisabledSingleSaveSessions">
          <children>
            <node id="SaveA">
              <attribute id="name" type="string" value="A"/>
              <attribute id="guid" type="guid" value="11111111-1111-1111-1111-111111111111"/>
            </node>
            <node id="SaveB">
              <attribute id="guid" type="guid" value="22222222-2222-2222-2222-222222222222"/>
            </node>
          </children>
        </node>
      </children>
    </node>
  </region>
</save>"#;

    const GUID_A: &str = "11111111-1111-1111-1111-111111111111";
    const GUID_B: &str = "22222222-2222-2222-2222-222222222222";

    // Build real LSF bytes from the synthetic LSX so the codec round-trip is exercised.
    fn synthetic_lsf() -> Vec<u8> {
        use larian_formats::lsf::LsfData;
        use std::io::Cursor;
        let data = LsfData::read_lsx(&mut Cursor::new(LSX.as_bytes())).unwrap();
        let mut out = Vec::new();
        data.write_lsf(&mut out).unwrap();
        out
    }

    #[test]
    fn lists_both_guids_in_order() {
        assert_eq!(list_guids(&synthetic_lsf()).unwrap(), vec![GUID_A, GUID_B]);
    }

    fn has_node_with_id(elem: &Element, id: &str) -> bool {
        if elem.name == "node" && elem.attributes.get("id").map(String::as_str) == Some(id) {
            return true;
        }
        elem.children.iter().any(|c| match c {
            XMLNode::Element(e) => has_node_with_id(e, id),
            _ => false,
        })
    }

    #[test]
    fn removes_only_the_selected_guid() {
        let out = reactivate(&synthetic_lsf(), GUID_A).unwrap();
        assert_eq!(list_guids(&out).unwrap(), vec![GUID_B]);
        let dom = lsf_to_dom(&out).unwrap();
        assert!(!has_node_with_id(&dom, "SaveA"));
        assert!(has_node_with_id(&dom, "SaveB"));
    }

    #[test]
    fn removes_last_guid_leaves_first() {
        let out = reactivate(&synthetic_lsf(), GUID_B).unwrap();
        assert_eq!(list_guids(&out).unwrap(), vec![GUID_A]);
    }

    #[test]
    fn removing_the_only_guid_leaves_empty() {
        let first = reactivate(&synthetic_lsf(), GUID_A).unwrap();
        let out = reactivate(&first, GUID_B).unwrap();
        assert_eq!(list_guids(&out).unwrap(), Vec::<String>::new());
        lsf_to_dom(&out).unwrap();
    }

    #[test]
    fn removing_unknown_guid_errors() {
        let err = reactivate(&synthetic_lsf(), "00000000-0000-0000-0000-000000000000").unwrap_err();
        assert!(err.to_string().contains("save not found"));
    }
}
