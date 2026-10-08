from pathlib import Path
import sys

def html(shape, n):
    if shape in ('backs','backparents','refs'):
        refs=''.join(f'<a href="#fn1" role="doc-noteref"'+(f' id="ref{i}"' if shape!='refs' else '')+'>1</a>' for i in range(n))
        backs=''.join((f'<span><a href="#ref{i}" class="footnote-back">back</a> </span>' if shape=='backparents' else f'<a href="#ref{i}" class="footnote-back">back</a>') for i in range(n)) if shape!='refs' else ''
        return '<p>Body '+refs+'</p><section><p id="fn1">Note.'+backs+'</p></section>'
    if shape in ('wrapper','containers-layout'):
        refs=''.join(f'<a href="#fn{i}" role="doc-noteref">1</a>' for i in range(n))
        notes=''.join((f'<div><p id="fn{i}">Note.</p></div> \n<!--layout-->' if shape=='containers-layout' else f'<p id="fn{i}">Note.</p>') for i in range(n))
        return '<p>'+refs+'</p><div id="endnotes">'+notes+'</div><p>Tail.</p>'
    if shape=='aliases':
        refs=''.join(f'<a href="#alias{i}" role="doc-noteref">1</a>' for i in range(n))
        targets=''.join(f'<a id="alias{i}" href="#unused" class="footnote-back">1</a>' for i in range(n))
        return '<p>'+refs+'</p><section><p>'+targets+'Note.</p></section>'
    if shape in ('long-inverse-class','long-inverse-ref-class'):
        refs='<a id="r" href="#fn1" role="doc-noteref">1</a>'*n
        role=' role="doc-noteref"' if shape=='long-inverse-class' else ''
        return '<p>'+refs+'</p><section><p>Note.<a id="fn1" href="#r" class="footnote-back '+('noise '*n)+'"'+role+'>back</a></p></section>'
    if shape=='duplicate-identities':
        return '<p>'+('<a id="r" href="#fn1" role="doc-noteref">1</a>'*n)+'</p><section><p id="fn1">Note.'+('<a href="#r" class="footnote-back">back</a>'*n)+'</p></section>'
    if shape in ('deep-shared-wrapper','deep-div-wrapper'):
        refs=''.join(f'<a href="#fn{i}" role="doc-noteref">1</a>' for i in range(n))
        notes=''.join(f'<p id="fn{i}">Note.</p>' for i in range(n))
        tag='object' if shape=='deep-shared-wrapper' else 'div'
        return '<p>'+refs+'</p><section>'+((f'<{tag}>')*n)+notes+((f'</{tag}>')*n)+'</section><p>Tail.</p>'
    if shape=='deep-alias-targets':
        refs=''.join(f'<a href="#alias{i}" role="doc-noteref">1</a>' for i in range(n))
        targets=''.join(f'<span><a id="alias{i}" href="#unused" class="footnote-back">1</a>' for i in range(n))
        return '<p>'+refs+'</p><section><p>'+targets+'Note.'+('</span>'*n)+'</p></section>'
    if shape=='nested-backlink-blocks':
        refs=''.join(f'<a id="r{i}" href="#fn{i}" role="doc-noteref">1</a>' for i in range(n))
        blocks=''.join(f'<div id="fn{i}">' for i in range(n))
        backs=''.join(f'<a href="#r{i}" class="footnote-back">back</a>' for i in range(n))
        return '<p>'+refs+'</p><section>'+blocks+'Note.'+backs+('</div>'*n)+'</section>'
    if shape=='separators':
        return '<p>Body<a href="#fn1" role="doc-noteref">1</a>.</p><section>'+('<hr> \n<!--layout-->'*n)+'<p id="fn1">Note.</p></section><p>Tail.</p>'
    if shape=='normal':
        refs=''.join(f'<a href="#fn{i}" id="ref{i}"><sup>{i}</sup></a>' for i in range(n))
        notes=''.join(f'<li id="fn{i}"><p>Note {i}.<a href="#ref{i}">back</a></p></li>' for i in range(n))
        return '<p>'+refs+'</p><section><ol>'+notes+'</ol></section>'
    if shape=='ordinary':
        return '<p>Ordinary <em>text</em> and <a href="https://example.com">link</a>.</p>'*n
    if shape=='note-admonition-titles': return '<p>'+''.join(f'<a href="#fn{i}" role="doc-noteref">1</a>' for i in range(1,n+1))+'</p><section>'+''.join(f'<div id="fn{i}"><aside class="admonition note" aria-labelledby="adm-{i}"><p class="admonition-title" id="adm-{i}">Title.</p><p>Body.</p></aside></div>' for i in range(1,n+1))+'</section>'
    if shape=='admonition-titles': return ''.join(f'<aside class="admonition note" aria-labelledby="adm-{i}"><p class="admonition-title" id="adm-{i}">Title.</p><p>Body.</p></aside>' for i in range(1,n+1))
    if shape=='blank-table-rows': return '<table>'+'<tr><td></td></tr>'*n+'<tr><td>x</td></tr></table>'
    if shape=='sections': return '<table><tbody><tr><td>x</td></tr></tbody>'+'<tbody></tbody>'*n+'</table>'
    if shape=='empty-code': return '<p>'+'<code></code>x'*n+'</p>'
    if shape=='nested-spans': return '<p><strong>'+'<strong>x</strong> y '*n+'</strong></p>'
    if shape=='nested-code-spans': return '<p><strong>'+'<strong><code>x</code></strong> y '*n+'</strong></p>'
    if shape=='hard-break-padding': return '<p>'+'<br> <em>x</em>'*n+'</p>'
    raise ValueError(shape)

if __name__=='__main__':
    sys.stdout.write(html(sys.argv[1],int(sys.argv[2])))
